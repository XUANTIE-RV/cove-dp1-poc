// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! COVI (CoVE Interrupt) SBI ECALL handler for AIA/IMSIC setup.
//!
//! ConvertImsic HPAs are stored in a per-hart array (imsic_hw_by_hart),
//! separate from vcpu fields. IMSIC G-stage mapping is deferred to
//! RunTvmVcpu first-run, when the actual execution hart is known.

use sbi_rt::CoveInterruptFunction;

use config::{MAX_HARTS, MAX_VCPUS};
use tvm::page_table::map_page;
use tvm::{get_hart_id, get_tvm_by_id, set_current_guest_id, verify_tvm_handle};
use utils::println;

/// The `(error, value)` pair for a call that did what it was asked.
const OK: (i64, i64) = (0, 0);

/// The `(error, value)` pair for a caller-supplied id the TSM cannot honour.
///
/// Every COVI function's error table names `SBI_ERR_INVALID_PARAM` for a bad
/// `tvm_guest_id` or `tvm_vcpu_id`, so an out-of-range id has to be reported
/// rather than absorbed.
fn invalid_param() -> (i64, i64) {
    (sbi_rt::Error::InvalidParam as i64, 0)
}

/// The `(error, value)` pair for a COVI function the TSM does not implement.
///
/// Reporting this is the honest answer for the unimplemented arms below. It is
/// safe to report here because no host path treats these as load-bearing: IMSIC
/// bind/unbind/rebind are used for moving a vCPU between physical CPUs, which this
/// single-vCPU POC never does, and the host bails out of those sequences on
/// error rather than proceeding as if they had succeeded.
fn not_supported() -> (i64, i64) {
    (sbi_rt::Error::NotSupported as i64, 0)
}

/// Handle COVI SBI calls for interrupt virtualization setup.
///
/// Returns the `(error, value)` pair for `sbiret`. Previously this returned `()`
/// and the caller had no way to report anything, so *every* COVI call answered
/// success — including out-of-range ids and the arms that do nothing at all. The
/// spec defines `SBI_ERR_INVALID_PARAM` / `SBI_ERR_INVALID_ADDRESS` for these
/// functions, and a host that is told "success" for a call the TSM ignored will
/// carry on as though the interrupt file were configured.
pub fn handle_cove_interrupt_msg(int_func: CoveInterruptFunction) -> (i64, i64) {
    use CoveInterruptFunction::*;
    match int_func {
        // Not implemented, but reported as success on purpose: the host treats a
        // failure here as fatal (`kvm_riscv_cove_aia_init` propagates it straight
        // out of AIA device init), so answering NOT_SUPPORTED would stop the TVM
        // from being created at all. That makes this a knowing fail-open — the
        // host believes the TVM's AIA parameters were accepted when nothing
        // validated or stored them. Closing it needs the real AiaInit, not an
        // error code.
        InitTvmAia {
            tvm_id: _,
            params_addr: _,
            len: _,
        } => OK,
        SetTvmAiaCpuImsicAddr {
            tvm_id,
            vcpu_id,
            imsic_addr,
        } => unsafe {
            let vcpu_idx = vcpu_id as usize;
            // `tvm_id` is an opaque handle; a stale one (destroyed TVM or
            // reused under a newer generation) must be refused.
            let gid = match verify_tvm_handle(tvm_id) {
                Ok(id) => id,
                Err(_) => return invalid_param(),
            };
            if vcpu_idx >= MAX_VCPUS {
                return invalid_param();
            }
            set_current_guest_id(gid);
            let tvm = &mut *get_tvm_by_id(gid);

            // spec: "This can be called only after sbi_covi_init_tvm_aia() and
            // before sbi_covh_finalize_tvm()", and its error table names the
            // state explicitly — INVALID_PARAM covers "the TVM wasn't in the
            // TVM_INITIALIZING state". Without the check a host could move a
            // running vCPU's interrupt file.
            //
            // (The "after init_tvm_aia" half cannot be enforced while AiaInit is
            // a no-op; see the note on that arm.)
            if !tvm.may_set_imsic_addr() {
                return invalid_param();
            }

            // spec: "No two vCPUs may share the same tvm_vcpu_imsic_gpa."
            // Sharing one interrupt file between vCPUs would cross their
            // interrupts. The scan skips this vCPU's own slot on purpose: the
            // host may re-issue the same address for the same vCPU, and
            // comparing against itself would reject a legitimate repeat.
            if tvm.imsic_gpa_claimed_by_other(vcpu_idx, imsic_addr) {
                return (sbi_rt::Error::InvalidAddress as i64, 0);
            }

            tvm.vcpus[vcpu_idx].imsic_gpa = imsic_addr;
            OK
        },
        ConvertAiaImsic { imsic_addr } => unsafe {
            let hart = get_hart_id();
            if hart >= MAX_HARTS {
                return invalid_param();
            }
            // ConvertImsic is hart-scoped (not TVM-scoped). Record the HPA
            // per hart in the TVM state so it can map IMSIC. Also complete
            // pending IMSIC setup for any vcpu pinned to this hart.
            let mut any_map_failed = false;
            let tvm = &mut *get_tvm_by_id(0);
            tvm.imsic_hw_by_hart[hart] = imsic_addr;
            for i in 0..MAX_VCPUS {
                if tvm.vcpus[i].pcpu_id == hart
                    && tvm.vcpus[i].imsic_gpa != 0
                    && tvm.vcpus[i].imsic_hw_addr == 0
                {
                    let gpa = tvm.vcpus[i].imsic_gpa;
                    tvm.vcpus[i].imsic_hw_addr = imsic_addr;
                    if !map_page(tvm.pgt_mode, tvm.pgt_root_addr, gpa, imsic_addr) {
                        // Locate the failure for the operator: the
                        // `pgt_pool exhausted` line printed inside
                        // `pgt_alloc_page` does not carry `vcpu`, and this
                        // loop can compress many failures into one FAILED
                        // return. Name the pair here so a partial success
                        // is diagnosable.
                        println!(
                            "[TSM] convert_imsic map failed: vcpu={} hart={} — pool exhausted",
                            i, hart
                        );
                        // Undo the imsic_hw_addr assignment so this vCPU
                        // reads as "not yet mapped" rather than "already
                        // mapped, skip the map step". The only recovery
                        // available today is `DestroyTvm`+recreate — the
                        // pool cannot grow after `AddTvmPageTablePages`
                        // (see `Tvm::init_pgt_pool` doc); the reset keeps
                        // per-vCPU state coherent for that path.
                        tvm.vcpus[i].imsic_hw_addr = 0;
                        any_map_failed = true;
                        continue;
                    }
                    core::arch::asm!("sfence.vma zero, zero");
                    riscv_regs::hfence_gvma!();
                }
            }
            if any_map_failed {
                return (sbi_rt::Error::Failed as i64, 0);
            }
            OK
        },
        ReclaimTvmAiaImsic { imsic_addr: _ } => not_supported(),
        BindAiaImsic {
            tvm_id,
            vcpu_id: _,
            imsic_mask: _,
        } => unsafe {
            // Mapping deferred to RunTvmVcpu first-run, but at least bind the
            // hart's current TVM context so subsequent calls touch the right
            // TVM state.
            //
            // Reported as success rather than NOT_SUPPORTED because the host
            // treats Bind as the prerequisite for running a vCPU: refusing it
            // would stop the guest from running at all, while the deferred
            // mapping does happen on first run.
            let gid = match verify_tvm_handle(tvm_id) {
                Ok(id) => id,
                Err(_) => return invalid_param(),
            };
            set_current_guest_id(gid);
            OK
        },
        InjectTvmCpu {
            tvm_id,
            vcpu_id,
            interrupt_id,
        } => unsafe {
            let vcpu_idx = vcpu_id as usize;
            let gid = match verify_tvm_handle(tvm_id) {
                Ok(id) => id,
                Err(_) => return invalid_param(),
            };
            if vcpu_idx >= MAX_VCPUS {
                return invalid_param();
            }
            set_current_guest_id(gid);
            let tvm = &mut *get_tvm_by_id(gid);
            let imsic_hw = tvm.vcpus[vcpu_idx].imsic_hw_addr;
            if imsic_hw == 0 {
                // No interrupt file bound yet. The spec has the TSM record the
                // interrupt and deliver it once the vCPU is bound; that is not
                // implemented, so the injection is dropped and the host is told
                // rather than left believing the interrupt was delivered.
                return invalid_param();
            }
            let imsic_setipnum = imsic_hw as *mut u32;
            core::ptr::write_volatile(imsic_setipnum, interrupt_id as u32);
            OK
        },
        UnbindAiaImsicBegin { .. }
        | UnbindAiaImsicEnd { .. }
        | RebindAiaImsicBegin { .. }
        | RebindAiaImsicClone { .. }
        | RebindAiaImsicEnd { .. } => not_supported(),
    }
}
