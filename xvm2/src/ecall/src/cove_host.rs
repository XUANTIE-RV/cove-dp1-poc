// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! CoVH (CoVE Host) SBI ECALL handler.

use core::sync::atomic::{AtomicBool, Ordering};
use core::{mem, ptr, slice};

use riscv_regs::{Readable, Writeable, CSR};
use sbi_rt::CoveHostFunction;
use utils::print::*;

use config::{MAX_VCPUS, PAGE_SIZE, TVM_VCPU_STATE_PAGES};
use tvm::guest_run::run_tvm_guest;
use tvm::page_meta::TransitionError;
use tvm::shmem::{self, Shmem};
use tvm::{
    current_vcpu_id, get_hart_id, get_tvm_by_id, set_current_guest_id, set_current_vcpu_id,
    verify_tvm_handle, Tvm, TvmError,
};
use vcpu::{resume, VcpuStatus};

/// Map a refused page-state transition onto the SBI error the host expects.
///
/// The spec's error sets for these calls only offer `INVALID_ADDRESS` and
/// `INVALID_PARAM`, so an out-of-range physical address reports the former and
/// every state or ownership violation reports the latter.
fn transition_error_to_sbi(err: TransitionError) -> i64 {
    match err {
        TransitionError::Untracked => sbi_rt::Error::InvalidAddress as i64,
        TransitionError::WrongState | TransitionError::WrongOwner | TransitionError::BadOwner => {
            sbi_rt::Error::InvalidParam as i64
        }
        // spec Table 24/25/26/42 (measured/zero/shared/imsic) do not list
        // `SBI_ERR_OUT_OF_PTPAGES`; `SBI_ERR_FAILED` is the closest allowed
        // code for "the operation could not complete for an internal
        // reason". Host receives a diagnostic line (`[TSM] pgt_pool
        // exhausted ...`) that names the real cause.
        TransitionError::PgtPoolExhausted => sbi_rt::Error::Failed as i64,
    }
}

/// SCAUSE value for supervisor ecall
const SCAUSE_SUPERVISOR_ECALL: u64 = 10;
/// SCAUSE value for virtual instruction
const SCAUSE_VIRTUAL_INSTRUCTION: u64 = 22;
/// SCAUSE value for load guest page fault
const SCAUSE_LOAD_GUEST_PAGE_FAULT: u64 = 21;
/// SCAUSE value for store/AMO guest page fault
const SCAUSE_STORE_GUEST_PAGE_FAULT: u64 = 23;
/// SCAUSE value for instruction guest page fault
const SCAUSE_INST_GUEST_PAGE_FAULT: u64 = 20;
/// SCAUSE value for supervisor timer interrupt (interrupt bit | cause 5)
const SCAUSE_SUPERVISOR_TIMER_IRQ: u64 = (1u64 << 63) | 5;
/// SCAUSE value for an illegal instruction exception
const SCAUSE_ILLEGAL_INSTRUCTION: u64 = 2;

/// `vsstatus.SIE` bit (supervisor interrupt enable).
const VSSTATUS_SIE: u64 = 1 << 1;
/// `vsstatus.SPIE` bit (previous supervisor interrupt enable).
const VSSTATUS_SPIE: u64 = 1 << 5;
/// `vsstatus.SPP` bit (previous privilege).
const VSSTATUS_SPP: u64 = 1 << 8;
/// `hstatus.SPVP` bit, the guest's effective previous privilege.
const HSTATUS_SPVP: u64 = 1 << 8;
/// Low bits of `vstvec` holding the trap-vector mode; masked off to get the base.
const VSTVEC_MODE_MASK: u64 = 0x3;

/// WFI instruction encoding
const WFI_INSTRUCTION: u64 = 0x10500073;

// ---------------------------------------------------------------------------
// Page-transition logging (first occurrence only)
//
// ConvertPages/ReclaimPages may fire hundreds of times under demand paging.
// To avoid console flooding, only the *first* successful convert and the
// *first* successful reclaim per TVM lifecycle are printed — that single HPA
// is sufficient for memtool isolation verification.  Flags are reset when
// a TVM is destroyed so the next TVM gets its own log line.
// ---------------------------------------------------------------------------

// Accessed only on the single-hart TSM dispatch path; atomics with
// `Relaxed` ordering keep access well-defined without a lock.
static CONVERT_LOGGED: AtomicBool = AtomicBool::new(false);
/// Set to true after the first RunTvmVcpu, so we only log demand-paging
/// converts that are guaranteed to be MPT-protected (RDSM only enforces MPT
/// after a TVM exists).
static TVM_RUNNING: AtomicBool = AtomicBool::new(false);

/// Reset page-log flags (call on TVM destroy so the next TVM gets fresh logs).
fn reset_page_log_flags() {
    CONVERT_LOGGED.store(false, Ordering::Relaxed);
    TVM_RUNNING.store(false, Ordering::Relaxed);
}

/// Fetch an instruction from guest memory using HLVX.
unsafe fn hlvx_fetch_instruction(guest_sepc: u64) -> u64 {
    let spvp_bit = 1u64 << 8;
    let old_hstatus = CSR.hstatus.get();
    CSR.hstatus.set(old_hstatus | spvp_bit);

    let mut insn: u64;
    core::arch::asm!(
        ".insn r 0x73, 0x4, 0x32, {rd}, {rs1}, x3",
        rd = out(reg) insn,
        rs1 = in(reg) guest_sepc,
    );
    insn &= 0xFFFF;
    if insn & 0x3 == 0x3 {
        let mut upper: u64;
        core::arch::asm!(
            ".insn r 0x73, 0x4, 0x32, {rd}, {rs1}, x3",
            rd = out(reg) upper,
            rs1 = in(reg) (guest_sepc + 2),
        );
        insn |= (upper & 0xFFFF) << 16;
    }

    CSR.hstatus.set(old_hstatus);
    insn
}

/// Handle the RunTvmVcpu SBI call — the main guest execution loop.
///
/// Returns the `(error, value)` pair for `sbiret`. A rejected call must
/// report a non-zero error: the host treats success as "the vCPU exited
/// normally" and immediately re-enters, so staying silent turns a destroyed
/// or not-yet-runnable TVM into an endless re-entry loop.
unsafe fn handle_tvm_cpu_run(guest_id: u64, vcpu_id: u64) -> (i64, i64) {
    let gid = match verify_tvm_handle(guest_id) {
        Ok(id) => id,
        Err(_) => {
            println!(
                "ERROR: RunTvmVcpu invalid or stale TVM handle 0x{:x}",
                guest_id
            );
            return (sbi_rt::Error::InvalidParam as i64, 0);
        }
    };
    set_current_guest_id(gid);
    let tvm = &mut *get_tvm_by_id(gid);
    // spec Table 28: `INVALID_PARAM` if the TVM is not runnable OR the
    // vcpu_id is invalid. The bounds check below reuses this arm.
    if tvm.state != Some(tvm::TvmState::Runnable) {
        println!(
            "ERROR: RunTvmVcpu on guest_id={} in state {:?}",
            guest_id, tvm.state
        );
        return (sbi_rt::Error::InvalidParam as i64, 0);
    }

    let vcpu_idx = vcpu_id as usize;
    if vcpu_idx >= MAX_VCPUS {
        println!("ERROR: vcpu_id {} exceeds MAX_VCPUS", vcpu_id);
        return (sbi_rt::Error::InvalidParam as i64, 0);
    }

    // spec §10.17: after a non-zero sbiret.value on a previous run,
    // subsequent runs with the same vcpu_id must fail. The termination
    // flag is set by the paths in the run loop that detect an
    // unrecoverable exit (shmem refused / corrupted resume record).
    //
    // The spec only says such a call "will fail"; it does not fix a
    // specific error code, and Table 28's `INVALID_PARAM` description
    // does not enumerate "vCPU is quarantined" among its causes. This
    // arm reads a quarantined `vcpu_id` as a form of "invalid vcpu_id"
    // — host behaviour is equivalent (a non-zero error prevents any
    // resume input from being applied), and the diagnostic line
    // distinguishes it from a plain out-of-range id.
    if !tvm.may_run_vcpu(vcpu_idx) {
        println!(
            "ERROR: RunTvmVcpu on quarantined vcpu (guest_id={} vcpu={})",
            guest_id, vcpu_idx
        );
        return (sbi_rt::Error::InvalidParam as i64, 0);
    }

    // Each vCPU is bound to one hart, so vcpu_id must match the physical
    // hart. If the host scheduler sends a vCPU to the wrong hart, reject it
    // cleanly rather than corrupting per-hart state.
    let hart = get_hart_id();
    if vcpu_idx != hart {
        println!(
            "ERROR: RunTvmVcpu vcpu_id={} on hart={} — vcpu/hart mismatch",
            vcpu_idx, hart
        );
        return (sbi_rt::Error::InvalidParam as i64, 0);
    }

    set_current_vcpu_id(vcpu_idx);

    let is_first_run = match tvm.vcpus[vcpu_idx].activate() {
        Ok(first) => first,
        Err(_) => {
            println!(
                "ERROR: sbi_covh_run_tvm_vcpu vcpu.activate() failed (guest_id={} vcpu={})",
                guest_id, vcpu_idx
            );
            return (sbi_rt::Error::Failed as i64, 0);
        }
    };

    if is_first_run {
        TVM_RUNNING.store(true, Ordering::Relaxed);
        let hart = get_hart_id();
        println!(
            "sbi_covh_run_tvm_vcpu first_run: guest_id={} vcpu={} hart={}",
            gid, vcpu_idx, hart
        );
        if !tvm.setup_vcpu_imsic(vcpu_idx, hart) {
            // pool exhausted while mapping IMSIC → refuse the run
            return (sbi_rt::Error::Failed as i64, 0);
        }
    } else {
        apply_host_resume_state(tvm);
    }

    guest_execution_loop(tvm);

    // spec §10.17: sbiret.value is 0 on a resumable exit, non-zero if the
    // vCPU cannot be resumed. `guest_execution_loop` sets the vCPU's
    // termination flag on unrecoverable conditions detected during the exit.
    let unrecoverable = tvm.vcpus[vcpu_idx].is_terminated();

    tvm.vcpus[vcpu_idx].deactivate();

    if unrecoverable {
        (0, 1)
    } else {
        (0, 0)
    }
}

/// Apply the host's answer to the exit this vCPU actually took.
///
/// Dispatch is driven by the TSM's own record of which exit the vCPU took.
/// The shared memory page is consulted for the fields that exit functionally
/// requires.
unsafe fn apply_host_resume_state(tvm: &mut tvm::Tvm) {
    let record = tvm.current_vcpu().pending_resume;

    let shmem_base =
        (tvm.shmem_addr + current_vcpu_id() as u64 * shmem::SHMEM_SIZE as u64) as *mut u8;
    let shm = Shmem::new(shmem_base, shmem::SHMEM_SIZE);

    // Dispatch on the recorded exit kind to determine what to read from shmem.
    match resume::authorized_input(record) {
        vcpu::ResumeInput::Nothing => {}
        vcpu::ResumeInput::EcallRet => {
            if !super::apply_guest_ecall_resume(tvm, &shm) {
                return;
            }
        }
        vcpu::ResumeInput::Gpr(rd) => {
            if let Some(val) = shm.read_gpr(rd as usize) {
                let vcpu_regs = tvm.vcpu_regs();
                let gprs_ptr = &raw mut vcpu_regs.guest_regs.gprs as *mut u64;
                *gprs_ptr.add(rd as usize) = val;
            } else {
                return;
            }
        }
    }

    if resume::advances_sepc(record.kind()) {
        tvm.vcpu_regs().guest_regs.sepc += record.insn_len as u64;
    }
}

unsafe fn guest_execution_loop(tvm: &mut tvm::Tvm) {
    let saved_stimecmp = riscv_regs::CSR.stimecmp.get();
    let saved_sie = riscv_regs::CSR.sie.get();

    // Preemption backstop: cap how long the guest may run in one RunTvmVcpu
    // slice before the timer forces an exit back to the host, in `time` ticks.
    const GUEST_PREEMPTION_BACKSTOP_TICKS: u64 = 10_000;
    let time: u64;
    core::arch::asm!("rdtime {}", out(reg) time);
    let deadline = time + GUEST_PREEMPTION_BACKSTOP_TICKS;
    riscv_regs::CSR.stimecmp.set(deadline);

    run_tvm_guest(tvm);
    let scause = CSR.scause.get();

    // Separate two things that used to be conflated: which scause the host is
    // told about, and whether the TSM already dealt with the trap itself.
    let reported_scause;
    let handled_internally;
    if handle_virtual_instruction(tvm, scause) {
        // Injected back into the guest; the host is told a benign reason and
        // must not be allowed to supply anything on resume.
        reported_scause = SCAUSE_SUPERVISOR_TIMER_IRQ;
        handled_internally = true;
    } else if handle_guest_page_fault_htinst_zero(tvm, scause) {
        // Only htinst was filled in via HLVX; the fault itself is forwarded.
        reported_scause = scause;
        handled_internally = false;
    } else if scause == SCAUSE_SUPERVISOR_ECALL && super::handle_guest_exit_ecall(tvm) {
        reported_scause = SCAUSE_SUPERVISOR_TIMER_IRQ;
        handled_internally = true;
    } else {
        reported_scause = scause;
        handled_internally = false;
    }

    // Record what we observed before handing the page to the host. Everything
    // here comes from real CSRs and this TVM's own bounds.
    let record = record_pending_resume(tvm, scause, handled_internally);
    if matches!(record.kind(), vcpu::ExitKind::Corrupted) {
        // spec §10.17: a corrupted resume record means the TSM cannot make
        // a safe resume decision for this vCPU. Mark unrecoverable so the
        // next RunTvmVcpu with the same vcpu_id is refused.
        println!(
            "[TSM] pending_resume corrupted on exit: vcpu={} — marking terminated",
            current_vcpu_id()
        );
        tvm.current_vcpu().mark_terminated();
    }

    // Collect the fault CSRs and write them back to the architectural
    // registers. Since `host_fault_csrs` passes all values through
    // (specification baseline), this is effectively a no-op on the CSR
    // content; it is kept so the local `visible` variable carries the values
    // to `write_exit_info_to_shmem` and the direct-read path after TEERET
    // always agrees with the shared memory page.
    let visible = resume::host_fault_csrs(
        record.kind(),
        CSR.htval.get(),
        CSR.stval.get(),
        CSR.htinst.get(),
    );
    CSR.htval.set(visible.htval);
    CSR.stval.set(visible.stval);
    CSR.htinst.set(visible.htinst);

    write_exit_info_to_shmem(tvm, reported_scause, record.kind(), visible);

    riscv_regs::CSR.stimecmp.set(saved_stimecmp);
    riscv_regs::CSR.sie.set(saved_sie);
}

/// Store the TSM's own account of this exit on the vCPU and return it.
///
/// Read from the real `htval`/`stval`/`htinst` and from the TVM's recorded
/// memory region, so nothing here can be influenced by the host. The resume path
/// consults only this record.
unsafe fn record_pending_resume(
    tvm: &mut tvm::Tvm,
    scause: u64,
    handled_internally: bool,
) -> vcpu::PendingResume {
    let htval = CSR.htval.get();
    let stval = CSR.stval.get();
    let htinst = CSR.htinst.get();
    let fault_gpa = (htval << 2) | (stval & 0x3);
    // Every declared region, not just the last one: the host classifies the same
    // fault against all of its memslots, and this decides whether the host may
    // supply a register value (MMIO emulation vs demand paging).
    let in_memslot = tvm.contains_gpa(fault_gpa);

    // Both flags come from this TVM's own tables: the host declared the RAM
    // regions, the guest declared its MMIO windows.
    let in_mmio_region = tvm.contains_mmio_gpa(fault_gpa);

    // The only failure mode of gating on a positive declaration: an access the
    // guest never declared is downgraded to a demand page, so the host gets a
    // page-aligned address and no instruction and the device access silently
    // stops working — which looks like a hang with no error line anywhere.
    // Bounded to a few prints because this sits on the exit path, where an
    // unbounded println is a known source of fmt-expansion panics under MTTCG.
    if !in_memslot
        && !in_mmio_region
        && matches!(
            scause,
            SCAUSE_LOAD_GUEST_PAGE_FAULT | SCAUSE_STORE_GUEST_PAGE_FAULT
        )
        && tvm.claim_undeclared_fault_report()
    {
        println!(
            "[TSM] UNDECLARED fault: gpa=0x{:x} scause={} sepc=0x{:x} (in neither RAM nor MMIO table -> demand page)",
            fault_gpa,
            scause,
            tvm.vcpu_regs().guest_regs.sepc
        );
    }

    let record = resume::classify_exit(
        scause,
        htinst,
        fault_gpa,
        in_memslot,
        in_mmio_region,
        handled_internally,
    );
    tvm.current_vcpu().pending_resume = record;
    record
}

unsafe fn handle_virtual_instruction(tvm: &mut tvm::Tvm, scause: u64) -> bool {
    if scause != SCAUSE_VIRTUAL_INSTRUCTION {
        return false;
    }

    let stval = CSR.stval.get();
    let vcpu_regs = tvm.vcpu_regs();
    let guest_sepc = vcpu_regs.guest_regs.sepc;
    let insn = if stval != 0 {
        stval
    } else {
        hlvx_fetch_instruction(guest_sepc)
    };

    if insn == WFI_INSTRUCTION {
        vcpu_regs.guest_regs.sepc += 4;
        return false;
    }

    inject_guest_trap(tvm, guest_sepc, insn);
    true
}

unsafe fn inject_guest_trap(tvm: &mut tvm::Tvm, guest_sepc: u64, trap_val: u64) {
    let vcpu_regs = tvm.vcpu_regs();
    let vs = &mut vcpu_regs.vs_csrs;
    let old_vsstatus = vs.vsstatus;

    vs.vsstatus &= !VSSTATUS_SIE;
    if old_vsstatus & VSSTATUS_SIE != 0 {
        vs.vsstatus |= VSSTATUS_SPIE;
    } else {
        vs.vsstatus &= !VSSTATUS_SPIE;
    }

    let hstatus = vcpu_regs.guest_regs.hstatus;
    if hstatus & HSTATUS_SPVP != 0 {
        vs.vsstatus |= VSSTATUS_SPP;
    } else {
        vs.vsstatus &= !VSSTATUS_SPP;
    }

    vs.vsepc = guest_sepc;
    vs.vscause = SCAUSE_ILLEGAL_INSTRUCTION;
    vs.vstval = trap_val;
    let new_sepc = vs.vstvec & !VSTVEC_MODE_MASK;
    vcpu_regs.guest_regs.sepc = new_sepc;
}

unsafe fn handle_guest_page_fault_htinst_zero(tvm: &mut tvm::Tvm, scause: u64) -> bool {
    let is_guest_page_fault = matches!(
        scause,
        SCAUSE_INST_GUEST_PAGE_FAULT | SCAUSE_LOAD_GUEST_PAGE_FAULT | SCAUSE_STORE_GUEST_PAGE_FAULT
    );
    if !is_guest_page_fault || CSR.htinst.get() != 0 {
        return false;
    }

    let guest_sepc = tvm.vcpu_regs().guest_regs.sepc;
    let insn = hlvx_fetch_instruction(guest_sepc);
    CSR.htinst.set(insn | 1);
    true
}

unsafe fn write_exit_info_to_shmem(
    tvm: &mut tvm::Tvm,
    scause: u64,
    _exit_kind: vcpu::ExitKind,
    visible: resume::HostFaultCsrs,
) {
    let shmem_addr = tvm.shmem_addr + (current_vcpu_id() as u64) * shmem::SHMEM_SIZE as u64;
    let shm = Shmem::new(shmem_addr as *mut u8, shmem::SHMEM_SIZE);

    // Write fault CSRs to shared memory.
    let mut delivered = shm.write_csr(shmem::CSR_SCAUSE_TRAP_CTRL, scause);
    delivered &= shm.write_csr(shmem::CSR_STVAL_TRAP_VAL, visible.stval);
    delivered &= shm.write_csr(shmem::CSR_HTVAL_TRAP_VAL, visible.htval);
    delivered &= shm.write_csr(shmem::CSR_HTINST_TRAP_INST, visible.htinst);

    // Write ALL 32 GPRs to shared memory unconditionally (specification baseline).
    let vcpu_regs = tvm.vcpu_regs();
    let gprs_ptr = &raw const vcpu_regs.guest_regs.gprs as *const u64;
    for i in 0..32usize {
        let val = *gprs_ptr.add(i);
        delivered &= shm.write_gpr(i, val);
    }

    if !delivered {
        println!(
            "[TSM] shmem write_exit_info partial failure: vcpu={} scause={} base=0x{:x}",
            current_vcpu_id(),
            scause,
            shmem_addr
        );
    }
}

/// Handle GetTsmInfo: construct and copy TsmInfo to host buffer.
///
/// # Safety
/// `dest_addr` is a host-provided physical address. The steps below check
/// alignment, sufficient length, and that every page the write touches is
/// tracked RAM in `NonConfidential` state. The RDSM/MPT domain-isolation
/// layer is a second line of defence.
unsafe fn handle_tsm_get_info(dest_addr: u64, len: u64) -> (i64, i64) {
    println!(
        "sbi_covh_get_tsm_info: addr=0x{:x},len=0x{:x}",
        dest_addr, len
    );
    let info_size = mem::size_of::<sbi_rt::TsmInfo>();

    // Step 1: non-null and 4-byte alignment check on the destination.
    if dest_addr == 0 || dest_addr % 4 != 0 {
        return (sbi_rt::Error::InvalidAddress as i64, 0);
    }
    // Step 2: caller-provided buffer must be large enough for `TsmInfo`.
    if (len as usize) < info_size {
        return (sbi_rt::Error::InvalidParam as i64, 0);
    }
    // Step 3: every page the copy will write must be tracked host RAM in
    // `NonConfidential` state. spec §10.2 requires `INVALID_ADDRESS` when
    // the destination is not accessible for the write; without this a
    // malicious host could aim `dest_addr` at a confidential page and rely
    // on the RDSM to bounce the store *after* the TSM had opened it.
    if !tvm::Tvm::check_host_writable_range(dest_addr, info_size as u64) {
        println!(
            "[TSM] REFUSE get_tsm_info: dest_addr=0x{:x} len=0x{:x} (not host-writable range)",
            dest_addr, info_size
        );
        return (sbi_rt::Error::InvalidAddress as i64, 0);
    }

    // Step 4: construct the spec v0.7 `TsmInfo` payload.
    //
    // Zero the backing storage BEFORE assigning fields: the v0.7 layout
    // has 4 bytes of padding at offset 12 (between `tsm_version` and the
    // 8-byte-aligned `tsm_capabilities`). A struct literal leaves that
    // padding uninitialized, and the whole struct is copied out to the
    // non-confidential host buffer below — uninitialized padding would
    // leak TSM stack memory to the untrusted host. `TsmInfo` is all
    // integer/enum fields (all-zero is a valid bit pattern), so
    // `mem::zeroed()` is sound here.
    let tsm_capabilities: u64 =
        (1u64 << sbi_rt::COVE_TSM_CAP_AIA) | (1u64 << sbi_rt::COVE_TSM_CAP_MEMORY_ALLOCATION);
    let tsm_version: u32 = (sbi_rt::TSM_VERSION_MAJOR << 16) | sbi_rt::TSM_VERSION_MINOR;
    let mut tsm_info: sbi_rt::TsmInfo = unsafe { mem::zeroed() };
    tsm_info.tsm_state = sbi_rt::TsmState::TsmReady;
    // spec Table 10 reserves IDs `>2` for future CoVE implementations, so a
    // private choice would clash if the working group later assigns that value
    // to someone else. Report `0` (unassigned) until we get an official ID.
    tsm_info.tsm_impl_id = sbi_rt::COVE_TSM_IMPL_UNASSIGNED;
    tsm_info.tsm_version = tsm_version;
    tsm_info.tsm_capabilities = tsm_capabilities;
    tsm_info.tvm_state_pages = MAX_VCPUS as u64 * shmem::SHMEM_PAGES_PER_VCPU;
    tsm_info.tvm_max_vcpus = MAX_VCPUS as u64;
    // Same constant now drives both what we report and what CreateTvmVcpu
    // checks the donated `state_page_addr` against, so those two cannot drift.
    // Per-vCPU shared memory state lives in the `tvm_state_addr` region,
    // strided by SHMEM_PAGES_PER_VCPU, not here. Lowering this to 0 would be
    // different reason: spec reads 0 as "TSM does not support the dynamic
    // memory allocation capability", which would contradict the
    // COVE_TSM_CAP_MEMORY_ALLOCATION bit set above.
    tsm_info.tvm_vcpu_state_pages = TVM_VCPU_STATE_PAGES;

    // Step 5: copy the structure out. Step 2 guarantees `len >= info_size`,
    // so the whole `TsmInfo` is always written (no truncation possible).
    let write_len = info_size;
    let tsm_info_bytes: &[u8] =
        unsafe { slice::from_raw_parts((&tsm_info as *const sbi_rt::TsmInfo).cast(), info_size) };
    let dest_ptr = dest_addr as *mut u8;
    unsafe {
        ptr::copy_nonoverlapping(tsm_info_bytes.as_ptr(), dest_ptr, write_len);
    }

    // Step 6: report the number of bytes written.
    (0, write_len as i64)
}

/// Main CoVH SBI ECALL handler — dispatches to per-command handlers.
/// Returns (error_code, return_value) for the SBI ecall response.
pub fn handle_cove_host_msg(host_func: CoveHostFunction) -> (i64, i64) {
    use CoveHostFunction::*;

    // spec: "The values htval and htinst are cleared by TSM on TEECALL".
    // Cleared before any handler runs, so a call that never enters the guest
    // cannot hand back stale fault state from an earlier exit.
    CSR.htval.set(0);
    CSR.htinst.set(0);

    match host_func {
        GetTsmInfo { dest_addr, len } => {
            return unsafe { handle_tsm_get_info(dest_addr, len) };
        }
        CreateTvm { params_addr, len } => {
            println!(
                "sbi_covh_create_tvm: params_addr=0x{:x}, len=0x{:x}",
                params_addr, len
            );

            // Step 1: params_addr must be non-null and 8-byte aligned
            // (TvmCreateParams contains two u64 fields, requiring 8-byte alignment).
            if params_addr == 0 || params_addr % 8 != 0 {
                return (sbi_rt::Error::InvalidAddress as i64, 0);
            }
            // Step 2: caller-provided buffer must cover the full TvmCreateParams struct.
            if (len as usize) < mem::size_of::<sbi_rt::TvmCreateParams>() {
                return (sbi_rt::Error::InvalidParam as i64, 0);
            }
            // Step 3: copy params to a local value to avoid holding a reference
            // to untrusted physical memory.
            // SAFETY: Step 1 guarantees `params_addr` is non-null and 8-byte
            // aligned (matching `TvmCreateParams`'s layout of two u64 fields).
            // Step 2 guarantees the buffer is at least `size_of::<TvmCreateParams>()`
            // bytes, so the read does not extend beyond the caller's buffer.
            let params = unsafe { ptr::read(params_addr as *const sbi_rt::TvmCreateParams) };

            // Step 4: tvm_page_directory_addr must be non-null and 16 KiB-aligned
            // (Sv48x4 root table is 4 contiguous pages = 16 KiB).
            if params.tvm_page_directory_addr == 0
                || params.tvm_page_directory_addr % (4 * PAGE_SIZE) != 0
            {
                return (sbi_rt::Error::InvalidAddress as i64, 0);
            }
            // Step 5: tvm_state_addr must be non-null and page-aligned
            // (holds TsmInfo::tvm_state_pages pages of TVM global state).
            if params.tvm_state_addr == 0 || params.tvm_state_addr % PAGE_SIZE != 0 {
                return (sbi_rt::Error::InvalidAddress as i64, 0);
            }
            // The region extents themselves are checked inside `Tvm::create`,
            // which requires both control regions to lie wholly within the
            // tracked RAM window before it writes a byte to either (see
            // `register_control_pages`). That is what bounds the zero-fill: a
            // host that under-allocates `tvm_state_addr` — whose required size
            // is `TsmInfo::tvm_state_pages`, now one full shmem region per vCPU —
            // gets the create refused instead of having the TSM write past the
            // end of its allocation.
            //
            // Still missing: "tracked RAM" says the pages are real RAM the TSM
            // accounts for, not that this particular host owns them. The state
            // check in `reserve_pages` (`AddShared` accepts only
            // NonConfidential/Shared) covers confidentiality, but there is no
            // per-host ownership notion in the MPT's two domains to check
            // against.

            // Step 6: all validations passed — commit state changes.
            // `create` returns an opaque handle (generation), which is
            // what the host must present on every later COVH/COVI call.
            match unsafe { Tvm::create(&params) } {
                Ok(handle) => {
                    println!(
                        "sbi_covh_create_tvm: guest_id=0x{:x}, pgt_root=0x{:x}, shmem=0x{:x}",
                        handle, params.tvm_page_directory_addr, params.tvm_state_addr
                    );
                    return (0, handle as i64);
                }
                Err(TvmError::NoFreeSlot) => return (sbi_rt::Error::Failed as i64, 0),
                // spec's Create TVM error set has no state-specific code; the
                // supplied control-page addresses are what is wrong here.
                // `BadMemoryRegion` cannot come from create — it is only produced
                // by `add_mem_region` — but map it to the same address error
                // rather than leaving the match open to a future variant.
                Err(TvmError::ControlPagesUnavailable) | Err(TvmError::BadMemoryRegion) => {
                    return (sbi_rt::Error::InvalidAddress as i64, 0)
                }
            }
        }
        DestroyTvm { guest_id } => {
            println!("sbi_covh_tvm_destroy: guest_id=0x{:x}", guest_id);
            // A stale handle (destroyed TVM or reused under a newer
            // generation) is refused here like everywhere else.
            let gid = match unsafe { verify_tvm_handle(guest_id) } {
                Ok(id) => id,
                Err(_) => return (sbi_rt::Error::InvalidParam as i64, 0),
            };
            unsafe {
                let tvm = &mut *get_tvm_by_id(gid);
                // spec: destroy verifies the VMM has stopped all virtual hart
                // *execution*; a Runnable vCPU occupies no hart, so only an
                // actively executing one blocks teardown.
                if tvm
                    .vcpus
                    .iter()
                    .any(|vcpu| vcpu.get_status() == VcpuStatus::Running)
                {
                    println!(
                        "sbi_covh_tvm_destroy: guest_id={} still executing vcpus",
                        gid
                    );
                    return (sbi_rt::Error::Failed as i64, 0);
                }

                // Reset page-log flags only after all pre-checks have passed
                // and the TVM is about to be destroyed. Neither an invalid
                // handle nor a still-running vCPU must reset audit state.
                reset_page_log_flags();
                set_current_guest_id(gid);
                tvm.destroy(gid);
            }
        }
        ConvertPages {
            page_addr,
            num_pages,
        } => {
            if page_addr == 0 || page_addr % PAGE_SIZE != 0 {
                return (sbi_rt::Error::InvalidAddress as i64, 0);
            }
            if num_pages == 0 {
                return (sbi_rt::Error::InvalidParam as i64, 0);
            }
            unsafe {
                if let Err(e) = Tvm::convert_pages(page_addr, num_pages) {
                    return (transition_error_to_sbi(e), 0);
                }
                if !CONVERT_LOGGED.load(Ordering::Relaxed) && TVM_RUNNING.load(Ordering::Relaxed) {
                    println!(
                        "TsmConvertPages (first demand-page): page_addr=0x{:x} num_pages={}",
                        page_addr, num_pages
                    );
                    CONVERT_LOGGED.store(true, Ordering::Relaxed);
                }
            }
        }
        ConvertSharedPages {
            page_addr,
            num_pages,
        } => {
            if page_addr == 0 || page_addr % PAGE_SIZE != 0 {
                return (sbi_rt::Error::InvalidAddress as i64, 0);
            }
            if num_pages == 0 {
                return (sbi_rt::Error::InvalidParam as i64, 0);
            }
            unsafe {
                if let Err(e) = Tvm::convert_shared_pages(page_addr, num_pages) {
                    return (transition_error_to_sbi(e), 0);
                }
            }
        }
        ReclaimPages {
            page_addr,
            num_pages,
        } => {
            if page_addr == 0 || page_addr % PAGE_SIZE != 0 {
                return (sbi_rt::Error::InvalidAddress as i64, 0);
            }
            if num_pages == 0 {
                return (sbi_rt::Error::InvalidParam as i64, 0);
            }
            // Refuses pages a live TVM still holds. Each page is zeroed before
            // its state is flipped back to NonConfidential, so the host cannot
            // read a previous TVM's plaintext once its MPT access is restored.
            unsafe {
                if let Err(e) = Tvm::reclaim_pages(page_addr, num_pages) {
                    return (transition_error_to_sbi(e), 0);
                }
            }
        }
        // No-op: single-hart edition, no remote TLB shootdown needed.
        InitiateGlobalFence => {}
        // No-op: single-hart edition, local fence is implicit.
        LocalFence => {}
        AddTvmPageTablePages {
            guest_id,
            page_addr,
            num_pages,
        } => {
            println!(
                "sbi_covh_add_pgt_pages: guest_id={}, page_addr=0x{:x}, num_pages={}",
                guest_id, page_addr, num_pages
            );
            let gid = match unsafe { verify_tvm_handle(guest_id) } {
                Ok(id) => id,
                Err(_) => return (sbi_rt::Error::InvalidParam as i64, 0),
            };
            unsafe {
                set_current_guest_id(gid);

                let tvm = &mut *get_tvm_by_id(gid);
                if let Err(e) = tvm.init_pgt_pool(gid, page_addr, num_pages) {
                    return (transition_error_to_sbi(e), 0);
                }
            }
        }
        AddTvmMemoryRegion {
            guest_id,
            guest_addr,
            len,
        } => {
            println!(
                "sbi_covh_add_mem_region: guest_id={}, guest_addr=0x{:x}, len=0x{:x}",
                guest_id, guest_addr, len
            );
            let gid = match unsafe { verify_tvm_handle(guest_id) } {
                Ok(id) => id,
                Err(_) => return (sbi_rt::Error::InvalidParam as i64, 0),
            };
            // spec Table 22 splits the error space: `region_len` invalid maps
            // to INVALID_PARAM, while `tvm_gpa_addr` invalid maps to
            // INVALID_ADDRESS. Without this pre-check length errors were
            // folded into the address arm below, so a host handing in a
            // zero-length or sub-page region got the wrong code back and
            // lost the signal that it had passed a bad parameter rather than
            // a bad address.
            if len == 0 || len % PAGE_SIZE != 0 {
                println!(
                    "[TSM] REFUSE add_mem_region: gid={} len=0x{:x} (zero or not page-multiple)",
                    gid, len
                );
                return (sbi_rt::Error::InvalidParam as i64, 0);
            }
            unsafe {
                set_current_guest_id(gid);

                let tvm = &mut *get_tvm_by_id(gid);
                // spec: "This call must not be made after calling
                // sbi_covh_finalize_tvm()". Without the check a host could keep
                // adding RAM regions to a running TVM, and because RAM wins over
                // MMIO in the fault classification that is a way to break the
                // guest's devices after it has started.
                if tvm.state != Some(tvm::TvmState::Initializing) {
                    println!(
                        "[TSM] REFUSE add_mem_region: gid={} state {:?} is not Initializing",
                        gid, tvm.state
                    );
                    return (sbi_rt::Error::InvalidParam as i64, 0);
                }
                if tvm.add_mem_region(guest_addr, len).is_err() {
                    println!(
                        "[TSM] REFUSE add_mem_region: gid={} gpa=0x{:x} len=0x{:x}",
                        gid, guest_addr, len
                    );
                    return (sbi_rt::Error::InvalidAddress as i64, 0);
                }
            }
        }
        AddTvmZeroPages {
            guest_id,
            page_addr,
            page_type: _,
            num_pages,
            guest_addr,
        } => {
            let gid = match unsafe { verify_tvm_handle(guest_id) } {
                Ok(id) => id,
                Err(_) => return (sbi_rt::Error::InvalidParam as i64, 0),
            };
            unsafe {
                set_current_guest_id(gid);

                let tvm = &mut *get_tvm_by_id(gid);
                // spec: "This call may be made only after calling
                // sbi_covh_finalize_tvm()". Zero pages are the demand-fault
                // path, so accepting them before finalization would let a host
                // grow the TVM's memory while its measurement is still open.
                //
                // Safe against the normal flow: the host's only call site is the
                // guest page-fault handler, and a fault requires a running vCPU,
                // which requires the TVM to be Runnable already.
                //
                // spec Table 25 has no state-specific code, so this reports
                // INVALID_PARAM like the other state rejections in this file.
                if !tvm.may_add_zero_pages() {
                    println!(
                        "[TSM] REFUSE add_zero_pages: gid={} state {:?} is not finalized",
                        gid, tvm.state
                    );
                    return (sbi_rt::Error::InvalidParam as i64, 0);
                }
                if let Err(e) = tvm.add_zero_pages(gid, page_addr, num_pages, guest_addr) {
                    return (transition_error_to_sbi(e), 0);
                }
            }
        }
        AddTvmMeasuredPages {
            guest_id,
            src_addr,
            dest_addr,
            page_type: _,
            num_pages,
            guest_addr,
        } => {
            let gid = match unsafe { verify_tvm_handle(guest_id) } {
                Ok(id) => id,
                Err(_) => return (sbi_rt::Error::InvalidParam as i64, 0),
            };
            // Note: full validation of `src_addr` (host RAM, page-aligned)
            // and `dest_addr` (confidential, page-aligned) is deferred to the
            // TVM implementation layer.
            // (spec §10.2 -> SBI_ERR_INVALID_ADDRESS on failure); see the
            // `# Safety` contract on `Tvm::add_measured_pages`.
            unsafe {
                set_current_guest_id(gid);

                let tvm = &mut *get_tvm_by_id(gid);
                if let Err(e) =
                    tvm.add_measured_pages(gid, src_addr, dest_addr, num_pages, guest_addr)
                {
                    return (transition_error_to_sbi(e), 0);
                }
            }
        }
        FinalizeTvm {
            guest_id,
            entry_sepc,
            entry_arg,
            tvm_identity_addr,
        } => {
            let gid = match unsafe { verify_tvm_handle(guest_id) } {
                Ok(id) => id,
                Err(_) => return (sbi_rt::Error::InvalidParam as i64, 0),
            };
            // spec §10.8 / Table 19: `tvm_identity_addr` is a pointer to a
            // 64-byte host-defined TVM identity. `0` omits the tvm-identity
            // claim; a non-zero value must be 64-byte aligned or the call
            // is rejected with INVALID_PARAM. This TSM does not yet
            // implement the attestation token that would carry the claim,
            // so any non-zero value is validated for alignment and then
            // dropped (with a log line) — accepting the ABI now means a
            // host that later starts passing an identity does not have to
            // wait for a second interface change once attestation lands.
            if tvm_identity_addr != 0 && tvm_identity_addr % 64 != 0 {
                println!(
                    "[TSM] REFUSE finalize_tvm: guest_id={} tvm_identity_addr=0x{:x} not 64-byte aligned",
                    guest_id, tvm_identity_addr
                );
                return (sbi_rt::Error::InvalidParam as i64, 0);
            }
            unsafe {
                let tvm = &mut *get_tvm_by_id(gid);
                // spec: FinalizeTvm transitions INITIALIZING -> RUNNABLE and
                // rejects a TVM that is not in the INITIALIZING state.
                if tvm.state != Some(tvm::TvmState::Initializing) {
                    return (sbi_rt::Error::InvalidParam as i64, 0);
                }
                set_current_guest_id(gid);

                tvm.finalize(entry_sepc, entry_arg);
                if tvm_identity_addr != 0 {
                    println!(
                        "sbi_covh_finalize_tvm: guest_id={} entry_sepc=0x{:x} tvm_identity_addr=0x{:x} (accepted for ABI, dropped — attestation not yet implemented)",
                        guest_id, entry_sepc, tvm_identity_addr
                    );
                } else {
                    println!(
                        "sbi_covh_finalize_tvm: guest_id={} entry_sepc=0x{:x}",
                        guest_id, entry_sepc
                    );
                }
            }
        }
        RunTvmVcpu { guest_id, vcpu_id } => unsafe {
            return handle_tvm_cpu_run(guest_id, vcpu_id);
        },
        PromoteToTvm { guest_id } => {
            println!(
                "[TSM] COVH PromoteToTvm: guest_id=0x{:x} => NotSupported",
                guest_id
            );
            return (sbi_rt::Error::NotSupported as i64, 0);
        }
        CreateTvmVcpu {
            guest_id,
            vcpu_id,
            state_page_addr,
        } => {
            let gid = match unsafe { verify_tvm_handle(guest_id) } {
                Ok(id) => id,
                Err(_) => return (sbi_rt::Error::InvalidParam as i64, 0),
            };
            let vcpu_idx = vcpu_id as usize;
            if vcpu_idx >= MAX_VCPUS {
                // spec Table 27: an invalid tvm_vcpu_id is SBI_ERR_INVALID_PARAM.
                // This used to fall through to the function's success return, so a
                // host asking for a vCPU the TSM cannot create was told it had one.
                println!(
                    "ERROR: sbi_covh_create_tvm_vcpu vcpu_id {} exceeds MAX_VCPUS (guest_id={})",
                    vcpu_id, guest_id
                );
                return (sbi_rt::Error::InvalidParam as i64, 0);
            }
            // spec `:4539-4542`: state_page_addr must be page-aligned, must point
            // to confidential memory, and must cover tvm_info.tvm_vcpu_state_pages
            // pages. The host's normal path (`alloc_pages` + `cove_convert_pages`
            // + size from `tsm_info`) satisfies all three, so this rejects only a
            // host that donated an address it did not actually convert. Table 27
            // uses INVALID_ADDRESS for a bad state page address, distinct from the
            // INVALID_PARAM above for a bad id.
            if !tvm::Tvm::check_vcpu_state_pages(state_page_addr, TVM_VCPU_STATE_PAGES) {
                println!(
                    "[TSM] REFUSE create_tvm_vcpu: guest_id={} vcpu_id={} state_page_addr=0x{:x} (misaligned, non-confidential, or wrong length)",
                    guest_id, vcpu_id, state_page_addr
                );
                return (sbi_rt::Error::InvalidAddress as i64, 0);
            }
            unsafe {
                set_current_guest_id(gid);

                let tvm = &mut *get_tvm_by_id(gid);
                // spec Table 27 `:4553-4555`: vCPUs may not be added after the
                // TVM is finalized. The measurement covers the vCPU list, so
                // accepting a new vCPU on a finalized TVM would grow measured
                // content behind the attestor's back.
                if !tvm.may_create_vcpu() {
                    println!(
                        "[TSM] REFUSE create_tvm_vcpu: gid={} state {:?} is not TVM_INITIALIZING",
                        gid, tvm.state
                    );
                    return (sbi_rt::Error::InvalidParam as i64, 0);
                }
                tvm.create_vcpu(vcpu_idx);
            }
            println!(
                "sbi_covh_create_tvm_vcpu: guest_id={} vcpu_id={}, state_page_addr=0x{:x}",
                guest_id, vcpu_id, state_page_addr
            );
        }
        AddTvmSharedPages {
            guest_id,
            page_addr,
            page_type: _,
            num_pages,
            guest_addr,
        } => {
            let gid = match unsafe { verify_tvm_handle(guest_id) } {
                Ok(id) => id,
                Err(_) => return (sbi_rt::Error::InvalidParam as i64, 0),
            };
            if page_addr == 0 || page_addr % PAGE_SIZE != 0 {
                return (sbi_rt::Error::InvalidAddress as i64, 0);
            }
            // Bound the batch like the other page-moving FIDs do: an unbounded
            // num_pages spins on the TSM's uninterruptible path and then panics
            // once the G-stage pool is exhausted.
            if num_pages == 0 || num_pages > config::TRACKED_PAGES as u64 {
                return (sbi_rt::Error::InvalidParam as i64, 0);
            }
            // The GPA needs the same alignment as the physical side: the
            // mapping loop derives every page from it, and an unaligned value
            // would be silently truncated to a VPN while the declared-region
            // check below reasons in bytes.
            if guest_addr % PAGE_SIZE != 0 {
                return (sbi_rt::Error::InvalidAddress as i64, 0);
            }
            unsafe {
                set_current_guest_id(gid);

                let tvm = &mut *get_tvm_by_id(gid);
                // spec §10.15: the target range "must lie within a region of
                // non-confidential memory previously defined by the TVM via
                // the guest interface". The shared-region table records those
                // declarations (COVG ShareMemory); a host mapping shared pages
                // into a GPA the guest never opted to share is refused.
                //
                // Check-then-act: the region lock is released before the
                // mapping runs, so a vCPU of this TVM on another hart can
                // unshare the range in between, leaving pages mapped shared
                // for a range no longer declared. Left as is because the
                // window needs the guest to revoke its own declaration mid
                // call — the host cannot open it alone — and the MMIO table's
                // fault-path gate has the same shape. Closing it means holding
                // the region lock across the mapping, which the `&mut self`
                // signature of `add_shared_pages` does not currently allow.
                let len = num_pages.saturating_mul(PAGE_SIZE);
                if !tvm.covers_shared_range(guest_addr, len) {
                    if tvm.claim_host_shared_refusal_report() {
                        println!(
                            "[TSM] REFUSE add_shared_pages: guest_addr=0x{:x} num_pages={} not within a guest-declared shared region",
                            guest_addr, num_pages
                        );
                        tvm.dump_shared_regions();
                    }
                    return (sbi_rt::Error::InvalidAddress as i64, 0);
                }
                if let Err(e) = tvm.add_shared_pages(gid, page_addr, num_pages, guest_addr) {
                    return (transition_error_to_sbi(e), 0);
                }
            }
        }
        // No-op: single-hart edition, no remote TLB shootdown needed.
        InitiateTvmFence { guest_id: _ } => {}
        TvmInvalidatePages {
            guest_id,
            guest_addr,
            len,
        } => {
            let gid = match unsafe { verify_tvm_handle(guest_id) } {
                Ok(id) => id,
                Err(_) => return (sbi_rt::Error::InvalidParam as i64, 0),
            };
            unsafe {
                set_current_guest_id(gid);
                let tvm = &mut *get_tvm_by_id(gid);
                if let Err(e) = tvm.block_pages(gid, guest_addr, len) {
                    return (transition_error_to_sbi(e), 0);
                }
            }
        }
        TvmValidatePages {
            guest_id,
            guest_addr,
            len,
        } => {
            let gid = match unsafe { verify_tvm_handle(guest_id) } {
                Ok(id) => id,
                Err(_) => return (sbi_rt::Error::InvalidParam as i64, 0),
            };
            unsafe {
                set_current_guest_id(gid);
                let tvm = &mut *get_tvm_by_id(gid);
                if let Err(e) = tvm.unblock_pages(gid, guest_addr, len) {
                    return (transition_error_to_sbi(e), 0);
                }
            }
        }
        TvmRemovePages {
            guest_id,
            guest_addr,
            len,
        } => {
            let gid = match unsafe { verify_tvm_handle(guest_id) } {
                Ok(id) => id,
                Err(_) => return (sbi_rt::Error::InvalidParam as i64, 0),
            };
            unsafe {
                set_current_guest_id(gid);
                let tvm = &mut *get_tvm_by_id(gid);
                if let Err(e) = tvm.remove_pages(gid, guest_addr, len) {
                    return (transition_error_to_sbi(e), 0);
                }
            }
        }
    }
    (0, 0)
}
