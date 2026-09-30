// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! What the TSM is willing to accept from the host when resuming a vCPU.
//!
//! The shared memory page is host-owned: the TSM writes exit information
//! into it and the host writes back whatever the guest needs to see. Dispatching
//! the resume on values *read back* from that page would let the host decide
//! which guest register to overwrite and how far to advance the guest's `sepc`,
//! without the guest having taken any trap at all.
//!
//! So the TSM records, at the moment it leaves the guest, what it actually
//! observed — from real CSRs — and on resume consults only that record. The
//! record names the single piece of host input this particular exit legitimately
//! needs; everything else in the page is ignored.
//!
//! This module is deliberately pure and free of target gating so the policy can
//! be exercised by host unit tests.

/// Why a vCPU left the guest, and what it therefore expects back on resume.
///
/// Stored inside `Vcpu`, which lives in a zero-initialized static array, so
/// `None` must own the all-zero encoding.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum ExitKind {
    /// Nothing is pending: either the vCPU has never run or its record was
    /// already consumed. Resume must apply nothing.
    None = 0,
    /// A guest ECALL the TSM forwarded to the host. The host answers with the
    /// SBI return pair.
    ForwardedEcall = 1,
    /// A load fault the host is expected to emulate. Exactly one guest register
    /// receives the value the host supplies.
    MmioLoad = 2,
    /// A store fault the host is expected to emulate. The store data already
    /// travelled outward, so nothing comes back.
    MmioStore = 3,
    /// A fault inside the TVM's memory region: the host adds a page and the
    /// guest re-executes the instruction untouched.
    DemandPage = 4,
    /// The TSM dealt with the trap itself and only reported a benign reason to
    /// the host. Resume must apply nothing.
    HandledInternally = 5,
    /// A load fault outside the memory region whose destination could not be
    /// decoded (`rd == x0` or an unfilled `htinst`). The host still needs the
    /// exact address and instruction to emulate the device access, but there is
    /// no register to inject into, so resume accepts nothing and the guest
    /// re-executes.
    MmioOpaque = 6,
    /// A stored discriminant that decoded to no known variant. The record byte
    /// lives in mutable per-vCPU state, so a corrupted value must accept
    /// nothing on resume.
    Corrupted = 255,
}

impl ExitKind {
    /// Decode a stored discriminant.
    ///
    /// Zero is the legitimate "nothing pending" encoding. Any other unknown
    /// value means the record byte was corrupted and resolves to
    /// [`ExitKind::Corrupted`], which accepts nothing on resume — fail-closed
    /// on the inbound path.
    pub const fn from_u8(raw: u8) -> Self {
        match raw {
            0 => ExitKind::None,
            1 => ExitKind::ForwardedEcall,
            2 => ExitKind::MmioLoad,
            3 => ExitKind::MmioStore,
            4 => ExitKind::DemandPage,
            5 => ExitKind::HandledInternally,
            6 => ExitKind::MmioOpaque,
            _ => ExitKind::Corrupted,
        }
    }
}

/// The TSM's own account of one guest exit.
///
/// `#[repr(C)]` and all-integer fields keep the all-zero bit pattern valid, which
/// the zero-initialized `TVM` instance relies on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct PendingResume {
    /// [`ExitKind`] discriminant; 0 means nothing pending.
    pub kind: u8,
    /// For [`ExitKind::MmioLoad`], the guest register the faulting instruction
    /// targets. Decoded by the TSM from the real `htinst`, never from the host.
    pub target_gpr: u8,
    /// Width of the faulting or trapping instruction, so `sepc` advances by an
    /// amount the TSM decided rather than one the host supplied.
    pub insn_len: u8,
    /// Guest physical address of the fault, recorded for diagnostics and to pin
    /// the emulation to the address that actually faulted.
    pub fault_gpa: u64,
}

impl PendingResume {
    /// An empty record: nothing pending, nothing accepted.
    pub const NONE: Self = PendingResume {
        kind: ExitKind::None as u8,
        target_gpr: 0,
        insn_len: 0,
        fault_gpa: 0,
    };

    /// The decoded exit reason.
    pub const fn kind(self) -> ExitKind {
        ExitKind::from_u8(self.kind)
    }
}

impl Default for PendingResume {
    fn default() -> Self {
        Self::NONE
    }
}

/// The single piece of host-provided state a resume may consume.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ResumeInput {
    /// Read nothing from the shared page and leave the guest context alone.
    Nothing,
    /// Read the SBI return pair and advance past the ECALL.
    EcallRet,
    /// Read exactly this one guest register's replacement value.
    Gpr(u8),
}

/// Whether this exit advances the guest's `sepc` on resume.
///
/// Emulated accesses have already been performed by the host, so the guest must
/// not re-execute them. A demand-paged fault is the opposite: the page now
/// exists and the same instruction has to run again.
pub const fn advances_sepc(kind: ExitKind) -> bool {
    matches!(
        kind,
        ExitKind::ForwardedEcall | ExitKind::MmioLoad | ExitKind::MmioStore
    )
}

/// Determine what the host should supply for a given exit (pure functional dispatch).
pub const fn authorized_input(record: PendingResume) -> ResumeInput {
    match record.kind() {
        ExitKind::ForwardedEcall => ResumeInput::EcallRet,
        ExitKind::MmioLoad => ResumeInput::Gpr(record.target_gpr),
        // A store's data left with the exit; a demand-paged fault re-executes;
        // an opaque MMIO load has no destination to inject into; other exits
        // have no input to consume.
        ExitKind::MmioStore
        | ExitKind::DemandPage
        | ExitKind::MmioOpaque
        | ExitKind::HandledInternally
        | ExitKind::Corrupted
        | ExitKind::None => ResumeInput::Nothing,
    }
}

/// Length of the trapping instruction, from the transformed `htinst`.
///
/// Bit 1 distinguishes a 32-bit instruction from a compressed 16-bit one.
pub const fn decode_insn_len(htinst: u64) -> u8 {
    if htinst & 0x2 != 0 {
        4
    } else {
        2
    }
}

/// Guest register index the faulting load writes, from the transformed `htinst`.
///
/// Returns `None` when the encoding is not usable: bit 0 clear means the field
/// was not filled in, and `x0` is not a writable destination.
pub const fn decode_load_rd(htinst: u64) -> Option<u8> {
    if htinst & 0x1 == 0 {
        return None;
    }
    let rd = ((htinst >> 7) & 0x1F) as u8;
    if rd == 0 {
        None
    } else {
        Some(rd)
    }
}

/// Build the record for one guest exit.
///
/// `scause` and `htinst` must come from the real CSRs. `in_memslot` and
/// `in_mmio_region` must come from the TVM's own region tables — the host
/// declares guest RAM, the guest declares its MMIO windows — never from
/// anything the host wrote into shared memory.
///
/// The two flags are consulted in order, and an address in neither table
/// authorises nothing. The spec permits the TSM to read or write the registers
/// involved in a fault only when the access is inside an emulated MMIO region,
/// so treating "outside guest RAM" as proof of a device access would grant that
/// to any address the guest never declared.
///
/// `handled_internally` marks the traps the TSM resolved itself and reported to
/// the host under a benign reason; those must accept nothing on resume even
/// though the host sees a plausible-looking exit.
pub fn classify_exit(
    scause: u64,
    htinst: u64,
    fault_gpa: u64,
    in_memslot: bool,
    in_mmio_region: bool,
    handled_internally: bool,
) -> PendingResume {
    const SCAUSE_SUPERVISOR_ECALL: u64 = 10;
    const SCAUSE_INST_GUEST_PAGE_FAULT: u64 = 20;
    const SCAUSE_LOAD_GUEST_PAGE_FAULT: u64 = 21;
    const SCAUSE_STORE_GUEST_PAGE_FAULT: u64 = 23;

    if handled_internally {
        return PendingResume {
            kind: ExitKind::HandledInternally as u8,
            target_gpr: 0,
            insn_len: 0,
            fault_gpa: 0,
        };
    }

    match scause {
        SCAUSE_SUPERVISOR_ECALL => PendingResume {
            kind: ExitKind::ForwardedEcall as u8,
            target_gpr: 0,
            // An ECALL is always a full-width instruction.
            insn_len: 4,
            fault_gpa: 0,
        },
        // An instruction fetch is never device emulation, so a fetch fault is
        // demand paging wherever it lands: the host gets the page number and
        // nothing else. Outside the memory region the host's add-page simply
        // fails — fail-closed, rather than disclosing the guest code that the
        // htinst-zero HLVX path has just placed into `htinst`.
        SCAUSE_INST_GUEST_PAGE_FAULT => PendingResume {
            kind: ExitKind::DemandPage as u8,
            target_gpr: 0,
            insn_len: 0,
            fault_gpa,
        },
        SCAUSE_LOAD_GUEST_PAGE_FAULT | SCAUSE_STORE_GUEST_PAGE_FAULT if in_memslot => {
            PendingResume {
                kind: ExitKind::DemandPage as u8,
                target_gpr: 0,
                insn_len: 0,
                fault_gpa,
            }
        }
        SCAUSE_LOAD_GUEST_PAGE_FAULT if in_mmio_region => match decode_load_rd(htinst) {
            Some(rd) => PendingResume {
                kind: ExitKind::MmioLoad as u8,
                target_gpr: rd,
                insn_len: decode_insn_len(htinst),
                fault_gpa,
            },
            // A genuine device load with no usable destination (rd == x0 or an
            // unfilled htinst). Not DemandPage: the host still needs the exact
            // fault address to emulate the access, it just gets nothing to
            // inject back. Keeping this distinct stops a misdecoded load from
            // silently switching the host to a page-aligned address.
            None => PendingResume {
                kind: ExitKind::MmioOpaque as u8,
                target_gpr: 0,
                insn_len: 0,
                fault_gpa,
            },
        },
        SCAUSE_STORE_GUEST_PAGE_FAULT if in_mmio_region => PendingResume {
            kind: ExitKind::MmioStore as u8,
            target_gpr: 0,
            insn_len: decode_insn_len(htinst),
            fault_gpa,
        },
        // A page fault in neither table. The guest never declared this address as
        // a device window, so nothing may be injected for it; report it as a
        // demand page so the guest re-executes instead. The host still sees the
        // fault and may map the address, after which the retry becomes an
        // ordinary demand-paging fault.
        SCAUSE_LOAD_GUEST_PAGE_FAULT | SCAUSE_STORE_GUEST_PAGE_FAULT => PendingResume {
            kind: ExitKind::DemandPage as u8,
            target_gpr: 0,
            insn_len: 0,
            fault_gpa,
        },
        // Interrupts and anything else the host merely observes.
        _ => PendingResume::NONE,
    }
}

/// The fault CSR values the host is allowed to see for a given exit.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct HostFaultCsrs {
    pub htval: u64,
    pub stval: u64,
    pub htinst: u64,
}

/// Return fault CSRs as-is without masking (specification baseline behavior).
///
/// All exit types pass through the raw htval/stval/htinst values to the host
/// unconditionally.
#[inline]
pub const fn host_fault_csrs(
    _kind: ExitKind,
    htval: u64,
    stval: u64,
    htinst: u64,
) -> HostFaultCsrs {
    HostFaultCsrs {
        htval,
        stval,
        htinst,
    }
}
