// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! TVM guest entry/exit logic.

use riscv_regs::{Readable, VmCpuRegisters, Writeable, CSR};

use super::tvm::Tvm;

/// HENVCFG bits needed for guest execution.
const HENVCFG_STCE: u64 = 1 << 63;
const HENVCFG_PBMTE: u64 = 1 << 62;
const HENVCFG_ADUE: u64 = 1 << 61;
const HENVCFG_CBZE: u64 = 1 << 7;
const HENVCFG_CBCFE: u64 = 1 << 6;
const HENVCFG_FIOM: u64 = 1 << 4;
const HENVCFG_NEEDED: u64 =
    HENVCFG_STCE | HENVCFG_PBMTE | HENVCFG_ADUE | HENVCFG_CBZE | HENVCFG_CBCFE | HENVCFG_FIOM;

/// HSTATUS bits.
const HSTATUS_SPV: u64 = 1 << 7;
const HSTATUS_SPVP: u64 = 1 << 8;
const HSTATUS_GVA: u64 = 1 << 12;
/// VTW (Virtual Trap WFI): causes guest WFI to trap as virtual instruction
/// (scause=22), allowing TSM to exit to host for proper vCPU halt/scheduling.
/// Without VTW, guest WFI suspends the hart until stimecmp fires, keeping the
/// host CPU in a tight TEECALL loop that starves RCU and the scheduler.
const HSTATUS_VTW: u64 = 1 << 21;
pub const HSTATUS_GUEST_BITS: u64 = HSTATUS_SPV | HSTATUS_SPVP | HSTATUS_GVA | HSTATUS_VTW;

extern "C" {
    fn _run_guest(state: *mut VmCpuRegisters);
    fn _save_vector(state: *mut VmCpuRegisters);
    fn _restore_vector(state: *mut VmCpuRegisters);
    fn _save_fp(state: *mut VmCpuRegisters);
    fn _restore_fp(state: *mut VmCpuRegisters);
}

/// Enter the TVM guest and return after a trap exit.
///
/// This function sets up HGATP, restores VS-mode CSRs, runs the guest,
/// then saves VS-mode CSRs back on exit.
///
/// # Safety
/// Must be called with valid TVM state and proper page table setup. The caller
/// passes the current TVM as `&mut Tvm`, so this function does not create a
/// second alias to the static TVM.
pub unsafe fn run_tvm_guest(tvm: &mut Tvm) {
    let henvcfg = CSR.henvcfg.get();
    if henvcfg & HENVCFG_NEEDED != HENVCFG_NEEDED {
        CSR.henvcfg.set(henvcfg | HENVCFG_NEEDED);
    }

    // Set HGATP before entering guest and flush ALL TLBs
    let hgatp = (tvm.pgt_mode.hgatp_mode() << 60) | (1u64 << 44) | (tvm.pgt_root_addr >> 12);
    CSR.hgatp.set(hgatp);
    core::arch::asm!("sfence.vma zero, zero");
    riscv_regs::hfence_gvma!();
    riscv_regs::hfence_vvma!();

    let vcpu_regs = tvm.vcpu_regs();
    vcpu_regs.guest_regs.hstatus |= HSTATUS_GUEST_BITS;

    // Restore VS-mode CSRs for the TVM guest.
    let vs = &vcpu_regs.vs_csrs;
    CSR.htimedelta.set(vs.htimedelta);
    CSR.vsstatus.set(vs.vsstatus);
    CSR.vsie.set(vs.vsie);
    CSR.vstvec.set(vs.vstvec);
    CSR.vsscratch.set(vs.vsscratch);
    CSR.vsepc.set(vs.vsepc);
    CSR.vscause.set(vs.vscause);
    CSR.vstval.set(vs.vstval);
    CSR.vsatp.set(vs.vsatp);
    CSR.vstimecmp.set(vs.vstimecmp);

    // Note: HVIP is NOT cleared here. KVM sets the correct hvip value
    // before each RunTvmVcpu via kvm_riscv_update_hvip(), and the value
    // survives the ecall→RDSM→TSM domain switch in hardware. Clearing
    // it here would discard KVM-injected virtual interrupts (VSEIP/VSSIP),
    // causing the guest to lose pending interrupts.

    // NOTE: stimecmp preemption backstop is set ONCE in guest_execution_loop(),
    // NOT here. Setting it here would reset the timer on every loop iteration,
    // defeating its purpose: when fast internally-handled ecalls (e.g. DBCN
    // WriteByte) cause the loop to `continue` without breaking, the stimecmp
    // would be perpetually pushed forward and never fire, trapping the host
    // CPU in the TSM domain indefinitely → RCU stall.

    // Enable HS-mode interrupts during TVM running so host can preempt:
    // - SSIE (bit 1): software interrupts (legacy IPI)
    // - STIE (bit 5): timer interrupts (stimecmp preemption)
    // - SEIE (bit 9): external interrupts (IMSIC/AIA IPI delivery)
    // Without SEIE, host IPIs (TLB flush, scheduler kick, RCU) sent via
    // IMSIC to this hart are never taken, causing remote CPUs to hang.
    CSR.sie.set((1 << 9) | (1 << 5) | (1 << 1));

    let regs_ptr = vcpu_regs as *mut VmCpuRegisters;
    _restore_fp(regs_ptr);
    _restore_vector(regs_ptr);
    _run_guest(regs_ptr);
    _save_vector(regs_ptr);
    _save_fp(regs_ptr);

    CSR.sie.set(0);

    // Restore HGATP to bare mode for the host, and flush TLB
    CSR.hgatp.set(0);
    riscv_regs::hfence_gvma!();

    // Save VS-mode CSRs back so they survive the round-trip
    let vs = &mut vcpu_regs.vs_csrs;
    vs.htimedelta = CSR.htimedelta.get();
    vs.vsstatus = CSR.vsstatus.get();
    vs.vsie = CSR.vsie.get();
    vs.vstvec = CSR.vstvec.get();
    vs.vsscratch = CSR.vsscratch.get();
    vs.vsepc = CSR.vsepc.get();
    vs.vscause = CSR.vscause.get();
    vs.vstval = CSR.vstval.get();
    vs.vsatp = CSR.vsatp.get();
    vs.vstimecmp = CSR.vstimecmp.get();
}
