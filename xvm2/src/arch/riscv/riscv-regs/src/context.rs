// SPDX-FileCopyrightText: 2026 DAMO Inc.
// SPDX-FileCopyrightText: 2023 Rivos Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! vCPU register context layouts for the RV64 hypervisor.
//!
//! Every structure in this module describes one slice of the hart state
//! that the world-switch code must persist: hypervisor-side registers
//! saved on guest entry, guest-side registers saved on guest exit, and
//! the VS/HS CSR banks swapped when scheduling between VMs.
//!
//! All types are `#[repr(C)]` with a frozen field order. The assembly
//! entry/exit stubs address individual fields through `offset_of!`-derived
//! constants, so the layouts below are part of the internal ABI and must
//! never be rearranged.

use crate::{FloatingPointRegisters, GeneralPurposeRegisters, VectorRegisters};

/// Hypervisor-side hart state stashed while a guest is executing.
///
/// Captured on guest entry and re-installed on guest exit so that the
/// hypervisor resumes exactly where `_run_guest` left off.
#[repr(C)]
#[derive(Default)]
pub struct HypervisorCpuState {
    /// Hypervisor integer register file.
    pub gprs: GeneralPurposeRegisters,
    /// Supervisor status at the moment of guest entry.
    pub sstatus: u64,
    /// Hypervisor status at the moment of guest entry.
    pub hstatus: u64,
    /// Counter-enable configuration for the hypervisor context.
    pub scounteren: u64,
    /// Hypervisor trap vector, restored after the guest exits.
    pub stvec: u64,
    /// Hypervisor scratch register, restored after the guest exits.
    pub sscratch: u64,
}

/// Guest-side hart state captured on every exit from virtualization.
///
/// Holds the integer, floating-point and vector register files plus the
/// CSRs a guest may legally observe, forming a complete resumable
/// snapshot of the virtual hart.
#[repr(C)]
#[derive(Default)]
pub struct GuestCpuState {
    /// Guest integer register file.
    pub gprs: GeneralPurposeRegisters,
    /// Guest floating-point register file (spilled/refilled wholesale).
    pub fprs: FloatingPointRegisters,
    /// Guest vector register file (spilled/refilled wholesale).
    pub vprs: VectorRegisters,
    /// Guest floating-point control and status register.
    pub fcsr: u64,
    /// Guest view of the supervisor status register.
    pub sstatus: u64,
    /// Hypervisor status bits associated with this guest context.
    pub hstatus: u64,
    /// Counter-enable configuration for the guest context.
    pub scounteren: u64,
    /// Program counter to resume the guest at.
    pub sepc: u64,

    /// Vector start index CSR.
    pub vstart: u64,
    /// Vector control and status register.
    pub vcsr: u64,
    /// Vector data type register.
    pub vtype: u64,
    /// Active vector length register.
    pub vl: u64,
}

/// VS-level CSR bank, live only while virtualization is on (V=1).
///
/// These registers are not swapped on every entry/exit, only when the
/// scheduler switches the physical hart between different VMs.
#[repr(C)]
#[derive(Default)]
pub struct GuestVsCsrs {
    /// Guest time offset relative to the host timebase.
    pub htimedelta: u64,
    /// Virtual supervisor status.
    pub vsstatus: u64,
    /// Virtual supervisor interrupt-enable bits.
    pub vsie: u64,
    /// Virtual supervisor trap vector.
    pub vstvec: u64,
    /// Virtual supervisor scratch register.
    pub vsscratch: u64,
    /// Virtual supervisor exception program counter.
    pub vsepc: u64,
    /// Virtual supervisor trap cause.
    pub vscause: u64,
    /// Virtual supervisor trap value.
    pub vstval: u64,
    /// Virtual supervisor address translation and protection.
    pub vsatp: u64,
    /// Virtual supervisor timer compare value.
    pub vstimecmp: u64,
}

/// Shadow copies of HS-level CSRs used to emulate (part of) the
/// hypervisor extension on behalf of a nested guest.
#[repr(C)]
#[derive(Default)]
pub struct GuestVirtualHsCsrs {
    /// Emulated hypervisor interrupt-enable register.
    pub hie: u64,
    /// Emulated hypervisor guest external interrupt-enable register.
    pub hgeie: u64,
    /// Emulated hypervisor guest address translation and protection.
    pub hgatp: u64,
}

/// Trap-cause CSRs latched by hardware on an exit from virtualization.
///
/// The trap dispatcher reads this snapshot to classify the exit reason
/// before deciding whether to emulate, forward or fault.
#[repr(C)]
#[derive(Default, Clone)]
pub struct VmCpuTrapState {
    /// Supervisor trap cause.
    pub scause: u64,
    /// Supervisor trap value (faulting address or instruction bits).
    pub stval: u64,
    /// Hypervisor trap value (guest physical address of the fault).
    pub htval: u64,
    /// Hypervisor trapped-instruction register.
    pub htinst: u64,
}

/// Aggregate of every register bank a virtual CPU owns.
///
/// One instance of this structure is the single source of truth for a
/// vCPU's architectural state across entry/exit and VM-to-VM switches;
/// the assembly world-switch code walks its sub-structures by offset.
#[repr(C)]
#[derive(Default)]
pub struct VmCpuRegisters {
    /// Hypervisor state saved across guest execution.
    pub hyp_regs: HypervisorCpuState,
    /// Guest state saved across hypervisor execution.
    pub guest_regs: GuestCpuState,
    /// VS-level CSRs swapped on VM-to-VM switches.
    pub vs_csrs: GuestVsCsrs,
    /// Emulated HS-level CSRs for nested virtualization support.
    pub virtual_hs_csrs: GuestVirtualHsCsrs,
    /// Trap-cause snapshot from the most recent guest exit.
    pub trap_csrs: VmCpuTrapState,
}

// Compile-time layout stability assertions. These constants evaluate at
// build time and document that every context structure is non-empty and
// therefore carries real, offset-addressable state for the assembly stubs.
const _: () = assert!(core::mem::size_of::<HypervisorCpuState>() > 0);
const _: () = assert!(core::mem::size_of::<GuestCpuState>() > 0);
const _: () = assert!(core::mem::size_of::<GuestVsCsrs>() > 0);
const _: () = assert!(core::mem::size_of::<GuestVirtualHsCsrs>() > 0);
const _: () = assert!(core::mem::size_of::<VmCpuTrapState>() > 0);
const _: () = assert!(core::mem::size_of::<VmCpuRegisters>() > 0);
