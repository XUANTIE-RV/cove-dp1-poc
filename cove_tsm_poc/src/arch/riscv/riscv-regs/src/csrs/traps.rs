// SPDX-FileCopyrightText: 2026 DAMO Inc.
// SPDX-FileCopyrightText: 2023 Rivos Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Strongly typed views of the RISC-V trap cause registers.
//!
//! A trap on RISC-V is reported through the `scause` CSR, whose layout is:
//!
//! ```text
//!   bit 63        bits 62..0
//!  +-----------+---------------------------+
//!  | interrupt |          reason           |
//!  +-----------+---------------------------+
//! ```
//!
//! When bit 63 is set the trap is an asynchronous [`Interrupt`]; when it is
//! clear the trap is a synchronous [`Exception`]. Both spaces use small
//! standard cause numbers in the low bits of the `reason` field. This module
//! wraps that encoding in the [`Trap`] enum so callers never compare raw
//! integers, and it provides checked conversions from cause values to the
//! per-cause bit positions used by the interrupt and delegation CSRs
//! (`sie`/`sip`, `hie`/`hip`/`hvip`, `hideleg`/`hedeleg`).
//!
//! Only the standard cause range is decoded; platform-defined cause values
//! are surfaced as [`Error::UnknownCause`] instead of being silently mapped.

use crate::{hedeleg, hideleg, hie, hip, hstatus, hvip, reason, scause, sie, sip};
use core::{fmt, result};
use tock_registers::fields::FieldValue;
use tock_registers::LocalRegisterCopy;

/// Failure modes of the cause-code conversions in this module.
#[derive(Copy, Clone, Debug)]
pub enum Error {
    /// The cause number read from a CSR is outside the standard set.
    UnknownCause(u64),

    /// The cause is standard but has no bit in the requested CSR.
    InvalidCause,
}

/// Convenience alias used by every fallible conversion below.
pub type Result<T> = result::Result<T, Error>;

/// Privilege mode that was executing when the trap was taken.
///
/// This mirrors the single-bit previous-privilege fields `sstatus.SPP` and
/// `hstatus.SPVP`; only U-mode and S-mode are representable there.
#[derive(Copy, Clone, Debug)]
#[repr(u64)]
pub enum PrivilegeLevel {
    User = 0,
    Supervisor = 1,
}

/// Interrupt cause numbers, i.e. `scause.reason` when `scause` bit 63 is set.
///
/// Variants are declared grouped by the privilege mode they target
/// (HS-mode, VS-mode, U-mode, M-mode); the wire encoding is fixed by the
/// explicit discriminant on each variant, not by declaration order.
#[derive(Copy, Clone, Debug)]
#[repr(u64)]
pub enum Interrupt {
    // Interrupts delivered to HS-mode.
    SupervisorSoft = 1,
    SupervisorTimer = 5,
    SupervisorExternal = 9,
    SupervisorGuestExternal = 12,
    // Interrupts delivered to a VS-mode guest.
    VirtualSupervisorSoft = 2,
    VirtualSupervisorTimer = 6,
    VirtualSupervisorExternal = 10,
    // Interrupts delivered to U-mode (N extension era; kept for decoding).
    UserSoft = 0,
    UserTimer = 4,
    UserExternal = 8,
    // Interrupts delivered to M-mode.
    MachineSoft = 3,
    MachineTimer = 7,
    MachineExternal = 11,
}

/// Exception cause numbers, i.e. `scause.reason` when `scause` bit 63 is clear.
///
/// Variants are declared grouped by fault category (instruction stream,
/// loads, stores, environment calls, guest second-stage faults); the wire
/// encoding is fixed by the explicit discriminant on each variant.
#[derive(Copy, Clone, Debug)]
#[repr(u64)]
pub enum Exception {
    // Instruction fetch and decode faults.
    InstructionMisaligned = 0,
    InstructionFault = 1,
    InstructionPageFault = 12,
    IllegalInstruction = 2,
    VirtualInstruction = 22,
    // Debug trap.
    Breakpoint = 3,
    // Load faults.
    LoadMisaligned = 4,
    LoadFault = 5,
    LoadPageFault = 13,
    // Store/AMO faults.
    StoreMisaligned = 6,
    StoreFault = 7,
    StorePageFault = 15,
    // Environment calls, one per originating privilege mode.
    UserEnvCall = 8,
    SupervisorEnvCall = 9,
    VirtualSupervisorEnvCall = 10,
    MachineEnvCall = 11,
    // Guest-page (second-stage translation) faults.
    GuestInstructionPageFault = 20,
    GuestLoadPageFault = 21,
    GuestStorePageFault = 23,
}

/// A decoded `scause` value: either an [`Interrupt`] or an [`Exception`].
#[derive(Copy, Clone, Debug)]
pub enum Trap {
    Interrupt(Interrupt),
    Exception(Exception),
}

// Compile-time safety net: the source-order grouping above must never drift
// from the architectural cause numbers.
const _: () = assert!(Interrupt::UserSoft as u64 == 0);
const _: () = assert!(Interrupt::SupervisorSoft as u64 == 1);
const _: () = assert!(Interrupt::VirtualSupervisorSoft as u64 == 2);
const _: () = assert!(Interrupt::MachineSoft as u64 == 3);
const _: () = assert!(Interrupt::UserTimer as u64 == 4);
const _: () = assert!(Interrupt::SupervisorTimer as u64 == 5);
const _: () = assert!(Interrupt::VirtualSupervisorTimer as u64 == 6);
const _: () = assert!(Interrupt::MachineTimer as u64 == 7);
const _: () = assert!(Interrupt::UserExternal as u64 == 8);
const _: () = assert!(Interrupt::SupervisorExternal as u64 == 9);
const _: () = assert!(Interrupt::VirtualSupervisorExternal as u64 == 10);
const _: () = assert!(Interrupt::MachineExternal as u64 == 11);
const _: () = assert!(Interrupt::SupervisorGuestExternal as u64 == 12);

const _: () = assert!(Exception::InstructionMisaligned as u64 == 0);
const _: () = assert!(Exception::InstructionFault as u64 == 1);
const _: () = assert!(Exception::IllegalInstruction as u64 == 2);
const _: () = assert!(Exception::Breakpoint as u64 == 3);
const _: () = assert!(Exception::LoadMisaligned as u64 == 4);
const _: () = assert!(Exception::LoadFault as u64 == 5);
const _: () = assert!(Exception::StoreMisaligned as u64 == 6);
const _: () = assert!(Exception::StoreFault as u64 == 7);
const _: () = assert!(Exception::UserEnvCall as u64 == 8);
const _: () = assert!(Exception::SupervisorEnvCall as u64 == 9);
const _: () = assert!(Exception::VirtualSupervisorEnvCall as u64 == 10);
const _: () = assert!(Exception::MachineEnvCall as u64 == 11);
const _: () = assert!(Exception::InstructionPageFault as u64 == 12);
const _: () = assert!(Exception::LoadPageFault as u64 == 13);
const _: () = assert!(Exception::StorePageFault as u64 == 15);
const _: () = assert!(Exception::GuestInstructionPageFault as u64 == 20);
const _: () = assert!(Exception::GuestLoadPageFault as u64 == 21);
const _: () = assert!(Exception::VirtualInstruction as u64 == 22);
const _: () = assert!(Exception::GuestStorePageFault as u64 == 23);

impl fmt::Display for Interrupt {
    fn fmt(&self, f: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        use Interrupt::*;
        match self {
            SupervisorSoft => write!(f, "supervisor software"),
            SupervisorTimer => write!(f, "supervisor timer"),
            SupervisorExternal => write!(f, "supervisor external"),
            SupervisorGuestExternal => write!(f, "supervisor guest external"),
            VirtualSupervisorSoft => write!(f, "virtual supervisor software"),
            VirtualSupervisorTimer => write!(f, "virtual supervisor timer"),
            VirtualSupervisorExternal => write!(f, "virtual supervisor external"),
            UserSoft => write!(f, "user software"),
            UserTimer => write!(f, "user timer"),
            UserExternal => write!(f, "user external"),
            MachineSoft => write!(f, "machine software"),
            MachineTimer => write!(f, "machine timer"),
            MachineExternal => write!(f, "machine external"),
        }
    }
}

impl fmt::Display for Exception {
    fn fmt(&self, f: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        use Exception::*;
        match self {
            InstructionMisaligned => write!(f, "instruction address misaligned"),
            InstructionFault => write!(f, "instruction access fault"),
            InstructionPageFault => write!(f, "instruction page fault"),
            IllegalInstruction => write!(f, "illegal instruction"),
            VirtualInstruction => write!(f, "virtual instruction"),
            Breakpoint => write!(f, "breakpoint"),
            LoadMisaligned => write!(f, "load address misaligned"),
            LoadFault => write!(f, "load access fault"),
            LoadPageFault => write!(f, "load page fault"),
            StoreMisaligned => write!(f, "store/AMO address misaligned"),
            StoreFault => write!(f, "store/AMO access fault"),
            StorePageFault => write!(f, "store/AMO page fault"),
            UserEnvCall => write!(f, "U-mode environment call"),
            SupervisorEnvCall => write!(f, "S-mode environment call"),
            VirtualSupervisorEnvCall => write!(f, "VS-mode environment call"),
            MachineEnvCall => write!(f, "M-mode environment call"),
            GuestInstructionPageFault => write!(f, "guest instruction page fault"),
            GuestLoadPageFault => write!(f, "guest load page fault"),
            GuestStorePageFault => write!(f, "guest store/AMO page fault"),
        }
    }
}

impl fmt::Display for Trap {
    fn fmt(&self, f: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        match self {
            Trap::Interrupt(i) => write!(f, "{i} interrupt"),
            Trap::Exception(e) => write!(f, "{e} exception"),
        }
    }
}

impl PrivilegeLevel {
    /// Extracts the pre-trap privilege mode from a raw `hstatus` value.
    ///
    /// Reads the single-bit `hstatus.SPVP` field, which records whether a
    /// trap taken into HS-mode came from VU-mode (0) or VS-mode (1).
    pub fn from_hstatus(csr: u64) -> Self {
        let hstatus_bits = LocalRegisterCopy::<u64, hstatus::Register>::new(csr);
        match hstatus_bits.read(hstatus::spvp) {
            0 => Self::User,
            1 => Self::Supervisor,
            _ => unreachable!(), // SPVP is a single bit; no other value exists.
        }
    }
}

impl Interrupt {
    /// Decodes a standard interrupt cause number into an [`Interrupt`].
    ///
    /// Only the low bits holding the standard cause set are inspected;
    /// any value outside that set yields [`Error::UnknownCause`].
    pub fn from_scause_reason(val: u64) -> Result<Self> {
        let cause_bits = LocalRegisterCopy::<u64, reason::Register>::new(val);
        match cause_bits.read(reason::std) {
            1 => Ok(Interrupt::SupervisorSoft),
            5 => Ok(Interrupt::SupervisorTimer),
            9 => Ok(Interrupt::SupervisorExternal),
            12 => Ok(Interrupt::SupervisorGuestExternal),
            2 => Ok(Interrupt::VirtualSupervisorSoft),
            6 => Ok(Interrupt::VirtualSupervisorTimer),
            10 => Ok(Interrupt::VirtualSupervisorExternal),
            0 => Ok(Interrupt::UserSoft),
            4 => Ok(Interrupt::UserTimer),
            8 => Ok(Interrupt::UserExternal),
            3 => Ok(Interrupt::MachineSoft),
            7 => Ok(Interrupt::MachineTimer),
            11 => Ok(Interrupt::MachineExternal),
            unknown => Err(Error::UnknownCause(unknown)),
        }
    }

    /// Maps this interrupt to its enable bit in `sie`.
    ///
    /// Only the HS-mode software/timer/external interrupts have an `sie`
    /// bit; every other cause returns [`Error::InvalidCause`].
    pub fn to_sie_field(&self) -> Result<FieldValue<u64, sie::Register>> {
        match self {
            Interrupt::SupervisorExternal => Ok(sie::sext.val(1)),
            Interrupt::SupervisorTimer => Ok(sie::stimer.val(1)),
            Interrupt::SupervisorSoft => Ok(sie::ssoft.val(1)),
            _ => Err(Error::InvalidCause),
        }
    }

    /// Maps this interrupt to its pending bit in `sip`.
    ///
    /// Mirrors [`Self::to_sie_field`]: only HS-mode causes are representable.
    pub fn to_sip_field(&self) -> Result<FieldValue<u64, sip::Register>> {
        match self {
            Interrupt::SupervisorExternal => Ok(sip::sext.val(1)),
            Interrupt::SupervisorTimer => Ok(sip::stimer.val(1)),
            Interrupt::SupervisorSoft => Ok(sip::ssoft.val(1)),
            _ => Err(Error::InvalidCause),
        }
    }

    /// Maps this interrupt to its delegation bit in `hideleg`.
    ///
    /// Only the VS-mode interrupt causes can be delegated to a guest.
    pub fn to_hideleg_field(&self) -> Result<FieldValue<u64, hideleg::Register>> {
        match self {
            Interrupt::VirtualSupervisorExternal => Ok(hideleg::vsext.val(1)),
            Interrupt::VirtualSupervisorTimer => Ok(hideleg::vstimer.val(1)),
            Interrupt::VirtualSupervisorSoft => Ok(hideleg::vssoft.val(1)),
            _ => Err(Error::InvalidCause),
        }
    }

    /// Maps this interrupt to its enable bit in `hie`.
    ///
    /// Covers the VS-mode causes plus the supervisor guest external
    /// interrupt, which is enabled through `hie` as well.
    pub fn to_hie_field(&self) -> Result<FieldValue<u64, hie::Register>> {
        match self {
            Interrupt::SupervisorGuestExternal => Ok(hie::sgext.val(1)),
            Interrupt::VirtualSupervisorExternal => Ok(hie::vsext.val(1)),
            Interrupt::VirtualSupervisorTimer => Ok(hie::vstimer.val(1)),
            Interrupt::VirtualSupervisorSoft => Ok(hie::vssoft.val(1)),
            _ => Err(Error::InvalidCause),
        }
    }

    /// Maps this interrupt to its pending bit in `hip`.
    ///
    /// Mirrors [`Self::to_hie_field`] for the pending side of the pair.
    pub fn to_hip_field(&self) -> Result<FieldValue<u64, hip::Register>> {
        match self {
            Interrupt::SupervisorGuestExternal => Ok(hip::sgext.val(1)),
            Interrupt::VirtualSupervisorExternal => Ok(hip::vsext.val(1)),
            Interrupt::VirtualSupervisorTimer => Ok(hip::vstimer.val(1)),
            Interrupt::VirtualSupervisorSoft => Ok(hip::vssoft.val(1)),
            _ => Err(Error::InvalidCause),
        }
    }

    /// Maps this interrupt to its injection bit in `hvip`.
    ///
    /// Only the three VS-mode causes may be injected into a guest via
    /// `hvip`; notably the supervisor guest external interrupt has no
    /// `hvip` bit and is rejected here.
    pub fn to_hvip_field(&self) -> Result<FieldValue<u64, hvip::Register>> {
        match self {
            Interrupt::VirtualSupervisorExternal => Ok(hvip::vsext.val(1)),
            Interrupt::VirtualSupervisorTimer => Ok(hvip::vstimer.val(1)),
            Interrupt::VirtualSupervisorSoft => Ok(hvip::vssoft.val(1)),
            _ => Err(Error::InvalidCause),
        }
    }
}

impl Exception {
    /// Decodes a standard exception cause number into an [`Exception`].
    ///
    /// Only the low bits holding the standard cause set are inspected;
    /// any value outside that set yields [`Error::UnknownCause`].
    pub fn from_scause_reason(val: u64) -> Result<Self> {
        let cause_bits = LocalRegisterCopy::<u64, reason::Register>::new(val);
        match cause_bits.read(reason::std) {
            0 => Ok(Exception::InstructionMisaligned),
            1 => Ok(Exception::InstructionFault),
            12 => Ok(Exception::InstructionPageFault),
            2 => Ok(Exception::IllegalInstruction),
            22 => Ok(Exception::VirtualInstruction),
            3 => Ok(Exception::Breakpoint),
            4 => Ok(Exception::LoadMisaligned),
            5 => Ok(Exception::LoadFault),
            13 => Ok(Exception::LoadPageFault),
            6 => Ok(Exception::StoreMisaligned),
            7 => Ok(Exception::StoreFault),
            15 => Ok(Exception::StorePageFault),
            8 => Ok(Exception::UserEnvCall),
            9 => Ok(Exception::SupervisorEnvCall),
            10 => Ok(Exception::VirtualSupervisorEnvCall),
            11 => Ok(Exception::MachineEnvCall),
            20 => Ok(Exception::GuestInstructionPageFault),
            21 => Ok(Exception::GuestLoadPageFault),
            23 => Ok(Exception::GuestStorePageFault),
            unknown => Err(Error::UnknownCause(unknown)),
        }
    }

    /// Maps this exception to its delegation bit in `hedeleg`.
    ///
    /// Causes that must always be handled by the hypervisor (HS-level
    /// environment calls, guest-page faults, virtual-instruction traps)
    /// have no delegation bit and return [`Error::InvalidCause`].
    pub fn to_hedeleg_field(&self) -> Result<FieldValue<u64, hedeleg::Register>> {
        match self {
            Exception::InstructionMisaligned => Ok(hedeleg::instr_misaligned.val(1)),
            Exception::InstructionFault => Ok(hedeleg::instr_fault.val(1)),
            Exception::InstructionPageFault => Ok(hedeleg::instr_page_fault.val(1)),
            Exception::IllegalInstruction => Ok(hedeleg::illegal_instr.val(1)),
            Exception::Breakpoint => Ok(hedeleg::breakpoint.val(1)),
            Exception::LoadMisaligned => Ok(hedeleg::load_misaligned.val(1)),
            Exception::LoadFault => Ok(hedeleg::load_fault.val(1)),
            Exception::LoadPageFault => Ok(hedeleg::load_page_fault.val(1)),
            Exception::StoreMisaligned => Ok(hedeleg::store_misaligned.val(1)),
            Exception::StoreFault => Ok(hedeleg::store_fault.val(1)),
            Exception::StorePageFault => Ok(hedeleg::store_page_fault.val(1)),
            Exception::UserEnvCall => Ok(hedeleg::u_ecall.val(1)),
            _ => Err(Error::InvalidCause),
        }
    }

    /// Returns true for the three guest-page (second-stage) fault causes.
    pub fn is_guest_page_fault(&self) -> bool {
        use Exception::*;
        matches!(
            self,
            GuestLoadPageFault | GuestStorePageFault | GuestInstructionPageFault
        )
    }
}

impl Trap {
    /// Decodes a raw `scause` CSR value into a [`Trap`].
    pub fn from_scause(csr: u64) -> Result<Self> {
        Self::try_from(LocalRegisterCopy::<u64, scause::Register>::new(csr))
    }

    /// Encodes this trap back into a raw `scause` CSR value.
    ///
    /// Interrupts set bit 63 alongside the cause number; exceptions leave
    /// it clear. This is the exact inverse of [`Self::from_scause`] for
    /// every representable cause.
    pub fn to_scause(self) -> u64 {
        let mut encoded = LocalRegisterCopy::<u64, scause::Register>::new(0);
        match self {
            Trap::Interrupt(i) => {
                encoded.modify(scause::is_interrupt.val(1));
                encoded.modify(scause::reason.val(i as u64));
            }
            Trap::Exception(e) => {
                encoded.modify(scause::reason.val(e as u64));
            }
        }
        encoded.get()
    }
}

impl TryFrom<LocalRegisterCopy<u64, scause::Register>> for Trap {
    type Error = Error;

    /// Splits an `scause` register copy on bit 63 and decodes the reason
    /// field through the matching cause-number decoder.
    fn try_from(val: LocalRegisterCopy<u64, scause::Register>) -> Result<Self> {
        if val.is_set(scause::is_interrupt) {
            Ok(Trap::Interrupt(Interrupt::from_scause_reason(
                val.read(scause::reason),
            )?))
        } else {
            Ok(Trap::Exception(Exception::from_scause_reason(
                val.read(scause::reason),
            )?))
        }
    }
}
