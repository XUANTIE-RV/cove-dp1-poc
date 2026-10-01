// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! COVG (CoVE Guest) SBI ECALL handler.
//!
//! Guest HartStart/HartStop/HartGetStatus are handled in TSM;
//! IPI and RFENCE are forwarded to host.

use riscv_regs::GprIndex;
use sbi_rt::{
    CoveGuestFunction, DbcnFunction, Error, HsmFunction, SbiMessage, SbiReturn, EXT_COVE_GUEST,
    EXT_DBCN, EXT_HSM,
};

use config::MAX_VCPUS;
use tvm::shmem::Shmem;
use tvm::Tvm;
#[cfg(feature = "tsm-debug")]
use utils::print::*;
use vcpu::VcpuStatus;

/// GPR register indices for shared memory scratch area access.
const REG_A0: usize = 10;
const REG_A1: usize = 11;

/// SBI HSM hart status values returned by HartGetStatus.
const HSM_STATUS_STARTED: i64 = 0;
const HSM_STATUS_STOPPED: i64 = 1;

/// Describes the action the guest execution loop should take after
/// processing a guest ecall.
enum GuestEcallAction {
    /// The ecall should be forwarded to the host via shared memory.
    /// The guest execution loop will call `write_exit_info_to_shmem()`
    /// and break out of the loop.
    ForwardToHost,
    /// The TSM has fully handled the ecall. The return value should be
    /// written back to the guest's a0/a1 registers and sepc advanced.
    #[allow(dead_code)]
    Handled(SbiReturn),
}

/// Top-level guest ecall classifier.
///
/// Routes by EID (a7):
/// - EXT_HSM → TSM-local HSM handling (HartStart/Stop/Status)
/// - EXT_COVE_GUEST → COVG dispatch
/// - All others (IPI, RFENCE, etc.) → forward to host
///
/// Returns `true` if handled internally, `false` to forward to host.
///
/// # Safety
/// Accesses TVM vCPU register state via per-hart indexing; the TVM must
/// be in a valid running state with its vCPU id set for this hart.
pub unsafe fn handle_guest_exit_ecall(tvm: &mut Tvm) -> bool {
    let gprs = &tvm.vcpu_regs().guest_regs.gprs;
    let eid = gprs.reg(GprIndex::A7);

    match eid {
        EXT_HSM => handle_guest_hsm(tvm),
        EXT_COVE_GUEST => handle_covg_ecall(tvm),
        EXT_DBCN => handle_dbcn_ecall(tvm),
        _ => false,
    }
}

/// Handle SBI DBCN (Debug Console) ecalls from the TVM guest.
///
/// The TSM proxies these calls to the firmware (RDSM/OpenSBI) via its own
/// SBI console, avoiding an unnecessary exit-to-Host round-trip for every
/// console output byte. This is safe because DBCN is a non-security-critical
/// debug facility and the TSM already uses the same console backend.
unsafe fn handle_dbcn_ecall(tvm: &mut Tvm) -> bool {
    let args = tvm.vcpu_regs().guest_regs.gprs.a_regs();
    let func = match DbcnFunction::from_regs(args) {
        Ok(f) => f,
        Err(_) => {
            write_ecall_return(tvm, SbiReturn::from(Error::NotSupported));
            return true;
        }
    };

    let ret = match func {
        DbcnFunction::WriteByte { byte } => {
            // Write a single byte via the TSM's own console driver, which
            // forwards to RDSM/OpenSBI through the standard SBI path.
            let msg = SbiMessage::PutChar(byte as u64);
            match sbi_rt::ecall_send::<()>(&msg) {
                Ok(()) => SbiReturn {
                    error_code: 0,
                    return_value: 1,
                },
                Err(_) => SbiReturn::from(Error::Io),
            }
        }
        DbcnFunction::Write {
            num_bytes,
            base_addr_lo,
            base_addr_hi,
        } => {
            // For multi-byte writes, we need to read from guest physical
            // memory. Since the TSM runs with HGATP set to the guest's
            // page table, we can use HLVX or direct access if the address
            // is in shared memory. For simplicity, fall back to Host for
            // buffer-based writes that require guest memory access.
            let _ = (num_bytes, base_addr_lo, base_addr_hi);
            return false; // ForwardToHost
        }
        DbcnFunction::Read { .. } => {
            // Console read requires Host-side handling (stdin/UART).
            return false; // ForwardToHost
        }
    };

    write_ecall_return(tvm, ret);
    true
}

/// Handle SBI HSM (Hart State Management) ecalls for guest hart lifecycle.
///
/// - HartStart: CAS PoweredOff→Runnable, initialize target vCPU regs, forward masked to host
/// - HartStop: set PoweredOff, forward to host (exit guest loop)
/// - HartGetStatus: return status directly without forwarding
unsafe fn handle_guest_hsm(tvm: &mut Tvm) -> bool {
    let args = tvm.vcpu_regs().guest_regs.gprs.a_regs();
    match HsmFunction::from_regs(args) {
        Ok(HsmFunction::HartStart {
            hartid,
            start_addr,
            opaque,
        }) => {
            let target = hartid as usize;
            if target >= MAX_VCPUS {
                write_ecall_return(tvm, SbiReturn::from(Error::InvalidParam));
                return true;
            }

            // Atomically PoweredOff → Runnable + initialize boot registers.
            if tvm.vcpus[target].power_on(start_addr, opaque).is_err() {
                write_ecall_return(tvm, SbiReturn::from(Error::AlreadyAvailable));
                return true;
            }

            // Mask start_addr and opaque before forwarding to host.
            // Host only needs the hartid to know which vCPU to schedule.
            tvm.current_vcpu().set_gpr(GprIndex::A1, 0);
            tvm.current_vcpu().set_gpr(GprIndex::A2, 0);
            false // forward masked HartStart to host
        }
        Ok(HsmFunction::HartStop) => {
            tvm.current_vcpu().set_status(VcpuStatus::PoweredOff);
            false // exit to host
        }
        Ok(HsmFunction::HartGetStatus { hartid }) => {
            let target = hartid as usize;
            if target >= MAX_VCPUS {
                write_ecall_return(tvm, SbiReturn::from(Error::InvalidParam));
                return true;
            }
            let st = tvm.vcpus[target].get_status();
            let status_val = match st {
                VcpuStatus::PoweredOff | VcpuStatus::Created => HSM_STATUS_STOPPED,
                VcpuStatus::Runnable | VcpuStatus::Running => HSM_STATUS_STARTED,
            };
            write_ecall_return(tvm, SbiReturn::success(status_val));
            true
        }
        Ok(HsmFunction::HartSuspend { .. }) => {
            write_ecall_return(tvm, SbiReturn::from(Error::NotSupported));
            true
        }
        Err(_) => {
            write_ecall_return(tvm, SbiReturn::from(Error::NotSupported));
            true
        }
    }
}

/// Parse and dispatch a COVG ecall by function ID.
///
/// On successful parse, delegates to `dispatch_covg()` which decides
/// whether to forward or handle locally. On unknown FID, returns
/// `SBI_ERR_NOT_SUPPORTED` directly to the guest without exiting to host.
unsafe fn handle_covg_ecall(tvm: &mut Tvm) -> bool {
    let args = tvm.vcpu_regs().guest_regs.gprs.a_regs();
    match CoveGuestFunction::from_regs(args) {
        Ok(guest_func) => match dispatch_covg(tvm, guest_func) {
            GuestEcallAction::ForwardToHost => false,
            GuestEcallAction::Handled(ret) => {
                write_ecall_return(tvm, ret);
                true
            }
        },
        Err(_) => {
            write_ecall_return(tvm, SbiReturn::from(Error::NotSupported));
            true
        }
    }
}

/// Per-FID COVG dispatch table.
///
/// Each arm explicitly names the COVG function for readability and auditability.
/// The two MMIO-region calls maintain the TSM's own region table first, because
/// the fault path needs to know which windows the guest declared before it will
/// expose the registers involved in a device access. Per spec both "will result in
/// an exit to the host on success", so a successful update forwards and a rejected
/// one answers the guest directly with `SBI_ERR_INVALID_ADDRESS` — the host is not
/// told about a declaration the TSM refused to honour.
fn dispatch_covg(tvm: &mut Tvm, func: CoveGuestFunction) -> GuestEcallAction {
    use CoveGuestFunction::*;
    match func {
        AddMmioRegion { addr, len } => match tvm.add_mmio_region(addr, len) {
            // Recorded *and* forwarded: the spec has this exit to the host on
            // success, so the TSM's table is a second copy, not a replacement.
            Ok(()) => GuestEcallAction::ForwardToHost,
            Err(_) => {
                // Refused means the fault path will not treat this window as MMIO.
                // Expected for a repeat declaration of a page already covered by an
                // earlier one — spec forbids overlapping regions, and sub-page PCI
                // BARs sharing a page make that routine.
                //
                // Bounded, for the same reason the exit path's undeclared-fault
                // print is: the guest drives this call and can repeat a refused
                // declaration indefinitely, and `println!` unwraps internally, so an
                // unbounded print here would hand the guest a way to panic the TSM.
                #[cfg(feature = "tsm-debug")]
                if tvm.claim_mmio_refusal_report() {
                    println!(
                        "[TSM] REFUSE add_mmio_region: addr=0x{:x} len=0x{:x} (overlaps an existing region, or malformed/table full)",
                        addr, len
                    );
                }
                GuestEcallAction::Handled(SbiReturn::from(sbi_rt::Error::InvalidAddress))
            }
        },
        RemoveMmioRegion { addr, len } => match tvm.remove_mmio_region(addr, len) {
            Ok(_removed) => GuestEcallAction::ForwardToHost,
            Err(_) => {
                #[cfg(feature = "tsm-debug")]
                if tvm.claim_mmio_refusal_report() {
                    println!(
                        "[TSM] REFUSE remove_mmio_region: addr=0x{:x} len=0x{:x} (no declared region overlaps it, or malformed)",
                        addr, len
                    );
                }
                GuestEcallAction::Handled(SbiReturn::from(sbi_rt::Error::InvalidAddress))
            }
        },
        // The share table maintains itself locally then forwards, mirroring the
        // MMIO arms: the host still performs the actual page-state transition,
        // the TSM's copy is what later authorises `AddTvmSharedPages` (spec
        // §10.15). Unlike MMIO, overlap and abutment merge instead of refuse —
        // the guest shares DMA buffers piecemeal, and answering an error to a
        // legitimate re-share fails `set_memory_decrypted` and kills the boot
        // (reproduced as `KVM_RUN Bad address`). What is still refused —
        // malformed ranges and a genuinely full table — is answered to the
        // guest, fail-closed, with a bounded diagnostic naming the range.
        ShareMemory { addr, len } => match tvm.add_shared_region(addr, len) {
            Ok(()) => GuestEcallAction::ForwardToHost,
            Err(_) => {
                #[cfg(feature = "tsm-debug")]
                if tvm.claim_shared_refusal_report() {
                    println!(
                        "[TSM] REFUSE share_memory: addr=0x{:x} len=0x{:x} (malformed, or table full)",
                        addr, len
                    );
                    tvm.dump_shared_regions();
                }
                GuestEcallAction::Handled(SbiReturn::from(sbi_rt::Error::InvalidParam))
            }
        },
        UnshareMemory { addr, len } => match tvm.remove_shared_region(addr, len) {
            Ok(_removed) => GuestEcallAction::ForwardToHost,
            // Nothing matched, malformed, or a both-sided split found no free
            // slot. Refusing is fail-closed for the last case: the range stays
            // declared, the guest is told the unshare did not happen.
            Err(_) => {
                #[cfg(feature = "tsm-debug")]
                if tvm.claim_shared_refusal_report() {
                    println!(
                        "[TSM] REFUSE unshare_memory: addr=0x{:x} len=0x{:x} (nothing declared there, malformed, or no slot to split)",
                        addr, len
                    );
                }
                GuestEcallAction::Handled(SbiReturn::from(sbi_rt::Error::InvalidParam))
            }
        },
        AllowExternalInterrupt { .. } => GuestEcallAction::ForwardToHost,
        DenyExternalInterrupt { .. } => GuestEcallAction::ForwardToHost,
    }
}

/// Write an SBI return value back to the guest's GPRs and advance sepc
/// past the ecall instruction.
unsafe fn write_ecall_return(tvm: &mut Tvm, ret: SbiReturn) {
    let vcpu_regs = tvm.vcpu_regs();
    vcpu_regs
        .guest_regs
        .gprs
        .set_reg(GprIndex::A0, ret.error_code as u64);
    vcpu_regs
        .guest_regs
        .gprs
        .set_reg(GprIndex::A1, ret.return_value as u64);
    vcpu_regs.guest_regs.sepc += 4;
}

/// Apply host resume state for a guest ecall exit.
///
/// Called from `apply_host_resume_state()` when the previous exit cause
/// was a guest ecall. Reads the SBI return values (a0, a1) from the
/// shared memory and writes them back into the guest's GPRs,
/// then advances sepc past the ecall instruction.
/// Returns `false` if the return pair could not be read, in which case nothing
/// was applied and the caller must not advance `sepc` — stepping past the ecall
/// carrying the previous exit's `a0`/`a1` would be worse than re-taking it.
///
/// # Safety
/// Accesses TVM vCPU register state via per-hart indexing and reads from
/// the shared-memory region. The TVM must be in a valid running state.
#[must_use]
pub unsafe fn apply_guest_ecall_resume(tvm: &mut Tvm, shm: &Shmem) -> bool {
    let (Some(ret_a0), Some(ret_a1)) = (shm.read_gpr(REG_A0), shm.read_gpr(REG_A1)) else {
        // Slot outside the region: apply nothing rather than a partial pair.
        return false;
    };
    let vcpu_regs = tvm.vcpu_regs();
    vcpu_regs.guest_regs.gprs.set_reg(GprIndex::A0, ret_a0);
    vcpu_regs.guest_regs.gprs.set_reg(GprIndex::A1, ret_a1);
    // `sepc` is advanced by the caller using the instruction length the TSM
    // recorded at exit, so every resume path moves the guest on by an amount
    // this side decided rather than one derived from host-supplied state.
    true
}
