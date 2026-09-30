// SPDX-FileCopyrightText: 2026 DAMO Inc.
// SPDX-FileCopyrightText: 2023 Rivos Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! CSR register access built on top of the tock-registers interface.
//!
//! The [`CSR`] singleton exposes one accessor per architectural CSR;
//! callers manipulate registers through the [`Readable`], [`Writeable`]
//! and [`RiscvCsrInterface`] traits re-exported below.

pub mod csr_access;
pub mod defs;
pub mod traps;

pub use tock_registers::interfaces::ReadWriteable;
pub use tock_registers::interfaces::Readable;
pub use tock_registers::interfaces::Writeable;
pub use tock_registers::LocalRegisterCopy;

pub use csr_access::RiscvCsrInterface;

pub use defs::*;
pub use traps::*;

use super::csr_addrs::*;
use csr_access::{IndirectReadWriteRiscvCsr, ReadWriteRiscvCsr};
use seq_macro::seq;

/// Shorthand for an indirect CSR reached through the S-mode
/// `siselect`/`sireg` window, selected by `V`.
type SiselectReg<R, const V: u64> = IndirectReadWriteRiscvCsr<
    R,
    ReadWriteRiscvCsr<siselect::Register, { CSR_S_ISELECT }>,
    ReadWriteRiscvCsr<sireg::Register, { CSR_S_IREG }>,
    V,
>;

/// Shorthand for an indirect CSR reached through the VS-mode
/// `vsiselect`/`vsireg` window, selected by `V`.
type VsiselectReg<R, const V: u64> = IndirectReadWriteRiscvCsr<
    R,
    ReadWriteRiscvCsr<siselect::Register, { CSR_VS_ISELECT }>,
    ReadWriteRiscvCsr<sireg::Register, { CSR_VS_IREG }>,
    V,
>;

pub struct CSR {
    // -- S-mode Supervisor registers (CSR 0x100..0x180) --
    /// Supervisor status (CSR 0x100): tracks interrupt and extension state.
    pub sstatus: ReadWriteRiscvCsr<sstatus::Register, { CSR_S_STATUS }>,
    /// Supervisor interrupt enable (CSR 0x104).
    pub sie: ReadWriteRiscvCsr<sie::Register, { CSR_S_IE }>,
    /// Supervisor trap vector base (CSR 0x105).
    pub stvec: ReadWriteRiscvCsr<stvec::Register, { CSR_S_TVEC }>,
    /// Supervisor counter enable (CSR 0x106).
    pub scounteren: ReadWriteRiscvCsr<scounteren::Register, { CSR_S_COUNTEREN }>,
    /// Supervisor scratch register (CSR 0x140).
    pub sscratch: ReadWriteRiscvCsr<sscratch::Register, { CSR_S_SCRATCH }>,
    /// Supervisor exception program counter (CSR 0x141).
    pub sepc: ReadWriteRiscvCsr<sepc::Register, { CSR_S_EPC }>,
    /// Supervisor cause register (CSR 0x142).
    pub scause: ReadWriteRiscvCsr<scause::Register, { CSR_S_CAUSE }>,
    /// Supervisor trap value (CSR 0x143).
    pub stval: ReadWriteRiscvCsr<stval::Register, { CSR_S_TVAL }>,
    /// Supervisor interrupt pending (CSR 0x144).
    pub sip: ReadWriteRiscvCsr<sip::Register, { CSR_S_IP }>,
    /// Supervisor timer compare (CSR 0x14D — Sstc extension).
    pub stimecmp: ReadWriteRiscvCsr<stimecmp::Register, { CSR_S_TIMECMP }>,
    /// S-mode external interrupt claim (CSR 0x15C — AIA).
    pub stopei: ReadWriteRiscvCsr<stopei::Register, { CSR_S_TOPEI }>,
    /// Supervisor address translation (CSR 0x180).
    pub satp: ReadWriteRiscvCsr<satp::Register, { CSR_S_ATP }>,
    /// S-mode top interrupt (CSR 0xDB0 — AIA).
    pub stopi: ReadWriteRiscvCsr<stopi::Register, { CSR_S_TOPI }>,

    // -- H-extension Hypervisor registers (CSR 0x600..0x680) --
    /// Hypervisor status (CSR 0x600).
    pub hstatus: ReadWriteRiscvCsr<hstatus::Register, { CSR_H_STATUS }>,
    /// Hypervisor exception delegation (CSR 0x602).
    pub hedeleg: ReadWriteRiscvCsr<hedeleg::Register, { CSR_H_EDELEG }>,
    /// Hypervisor interrupt delegation (CSR 0x603).
    pub hideleg: ReadWriteRiscvCsr<hideleg::Register, { CSR_H_IDELEG }>,
    /// Hypervisor interrupt enable (CSR 0x604).
    pub hie: ReadWriteRiscvCsr<hie::Register, { CSR_H_IE }>,
    /// Hypervisor counter enable (CSR 0x606).
    pub hcounteren: ReadWriteRiscvCsr<hcounteren::Register, { CSR_H_COUNTEREN }>,
    /// Hypervisor guest external interrupt enable (CSR 0x607).
    pub hgeie: ReadWriteRiscvCsr<hgeie::Register, { CSR_H_GEIE }>,
    /// Hypervisor virtual interrupt control (CSR 0x609 — AIA).
    pub hvictl: ReadWriteRiscvCsr<hvictl::Register, { CSR_H_VICTL }>,
    /// Hypervisor trap value (CSR 0x643).
    pub htval: ReadWriteRiscvCsr<htval::Register, { CSR_H_TVAL }>,
    /// Hypervisor interrupt pending (CSR 0x644).
    pub hip: ReadWriteRiscvCsr<hip::Register, { CSR_H_IP }>,
    /// Hypervisor virtual interrupt pending (CSR 0x645).
    pub hvip: ReadWriteRiscvCsr<hvip::Register, { CSR_H_VIP }>,
    /// Hypervisor trap instruction (CSR 0x64A).
    pub htinst: ReadWriteRiscvCsr<htinst::Register, { CSR_H_TINST }>,
    /// Hypervisor guest external interrupt pending (CSR 0xE12).
    pub hgeip: ReadWriteRiscvCsr<hgeip::Register, { CSR_H_GEIP }>,
    /// Hypervisor environment configuration (CSR 0x60A).
    pub henvcfg: ReadWriteRiscvCsr<henvcfg::Register, { CSR_H_ENVCFG }>,
    /// Hypervisor state enable bits (CSR 0x60C — Smstateen).
    pub hstateen0: ReadWriteRiscvCsr<hstateen0::Register, { CSR_H_STATEEN0 }>,
    /// Hypervisor guest address translation (CSR 0x680).
    pub hgatp: ReadWriteRiscvCsr<hgatp::Register, { CSR_H_GATP }>,
    /// Hypervisor time delta (CSR 0x605).
    pub htimedelta: ReadWriteRiscvCsr<htimedelta::Register, { CSR_H_TIMEDELTA }>,
    // VCoVE: seed CSR emulation support.
    pub seed: ReadWriteRiscvCsr<seed::Register, CSR_SEED>,

    // -- VS-mode Virtual Supervisor registers (CSR 0x200..0x280) --
    /// Virtual supervisor status (CSR 0x200).
    pub vsstatus: ReadWriteRiscvCsr<sstatus::Register, { CSR_VS_STATUS }>,
    /// Virtual supervisor interrupt enable (CSR 0x204).
    pub vsie: ReadWriteRiscvCsr<sie::Register, { CSR_VS_IE }>,
    /// Virtual supervisor trap vector (CSR 0x205).
    pub vstvec: ReadWriteRiscvCsr<stvec::Register, { CSR_VS_TVEC }>,
    /// Virtual supervisor scratch (CSR 0x240).
    pub vsscratch: ReadWriteRiscvCsr<sscratch::Register, { CSR_VS_SCRATCH }>,
    /// Virtual supervisor exception PC (CSR 0x241).
    pub vsepc: ReadWriteRiscvCsr<sepc::Register, { CSR_VS_EPC }>,
    /// Virtual supervisor cause (CSR 0x242).
    pub vscause: ReadWriteRiscvCsr<scause::Register, { CSR_VS_CAUSE }>,
    /// Virtual supervisor trap value (CSR 0x243).
    pub vstval: ReadWriteRiscvCsr<stval::Register, { CSR_VS_TVAL }>,
    /// Virtual supervisor interrupt pending (CSR 0x244).
    pub vsip: ReadWriteRiscvCsr<sip::Register, { CSR_VS_IP }>,
    /// Virtual supervisor timer compare (CSR 0x24D — Sstc).
    pub vstimecmp: ReadWriteRiscvCsr<stimecmp::Register, { CSR_VS_TIMECMP }>,
    /// VS-mode external interrupt claim (CSR 0x25C — AIA).
    pub vstopei: ReadWriteRiscvCsr<stopei::Register, { CSR_VS_TOPEI }>,
    /// Virtual supervisor address translation (CSR 0x280).
    pub vsatp: ReadWriteRiscvCsr<satp::Register, { CSR_VS_ATP }>,
    /// VS-mode top interrupt (CSR 0xEB0 — AIA).
    pub vstopi: ReadWriteRiscvCsr<stopi::Register, { CSR_VS_TOPI }>,

    // -- Vector extension state registers (CSR 0x008..0xC22) --
    /// Vector start position (CSR 0x008).
    pub vstart: ReadWriteRiscvCsr<vstart::Register, CSR_VSTART>,
    /// Vector CSR (CSR 0x00F).
    pub vcsr: ReadWriteRiscvCsr<vcsr::Register, CSR_VCSR>,
    /// Vector length (CSR 0xC20, read-only).
    pub vl: ReadWriteRiscvCsr<vl::Register, CSR_VL>,
    /// Vector type (CSR 0xC21, read-only).
    pub vtype: ReadWriteRiscvCsr<vtype::Register, CSR_VTYPE>,
    /// VLEN/8, vector register width in bytes (CSR 0xC22, read-only).
    pub vlenb: ReadWriteRiscvCsr<vlenb::Register, CSR_VLENB>,

    // -- Performance counters (CSR 0xC00..0xC1F) --
    pub hpmcounter: [&'static dyn RiscvCsrInterface<R = hpmcounter::Register>; 32],

    // -- S-mode AIA/IMSIC indirect registers (via siselect/sireg) --
    pub si_eidelivery: SiselectReg<eidelivery::Register, ISELECT_EIDELIVERY>,
    pub si_eithreshold: SiselectReg<eithreshold::Register, ISELECT_EITHRESHOLD>,

    // Per the AIA spec, RV64 only implements the even-numbered eip and
    // eie registers (keeping the numbering compatible with RV32, where
    // the odd-numbered registers expose the upper 32 bits). Rather than
    // mirroring that quirk, an array of 32 registers of 64-bit width is
    // declared here, which is more ergonomic to work with.
    pub si_eip: [&'static dyn RiscvCsrInterface<R = eip::Register>; 32],
    pub si_eie: [&'static dyn RiscvCsrInterface<R = eie::Register>; 32],

    // -- VS-mode AIA/IMSIC indirect registers (via vsiselect/vsireg) --
    pub vsi_eidelivery: VsiselectReg<eidelivery::Register, ISELECT_EIDELIVERY>,
    pub vsi_eithreshold: VsiselectReg<eithreshold::Register, ISELECT_EITHRESHOLD>,

    pub vsi_eip: [&'static dyn RiscvCsrInterface<R = eip::Register>; 32],
    pub vsi_eie: [&'static dyn RiscvCsrInterface<R = eie::Register>; 32],
}

// Standalone accessor constants for the registers referenced several
// times below, keeping the initializer readable.
const SISELECT: ReadWriteRiscvCsr<siselect::Register, { CSR_S_ISELECT }> = ReadWriteRiscvCsr::new();
const SIREG: ReadWriteRiscvCsr<sireg::Register, { CSR_S_IREG }> = ReadWriteRiscvCsr::new();
const VSISELECT: ReadWriteRiscvCsr<siselect::Register, { CSR_VS_ISELECT }> =
    ReadWriteRiscvCsr::new();
const VSIREG: ReadWriteRiscvCsr<sireg::Register, { CSR_VS_IREG }> = ReadWriteRiscvCsr::new();

// Bind every CSR accessor to its architectural register number.
// The lint suppressions below silence false positives triggered by the
// seq! counter arithmetic; see the clippy issue filed at
// https://github.com/rust-lang/rust-clippy/issues/10230
#[allow(clippy::identity_op, clippy::erasing_op)]
pub const CSR: &CSR = &CSR {
    // Hypervisor (H-extension) registers.
    hstatus: ReadWriteRiscvCsr::new(),
    hedeleg: ReadWriteRiscvCsr::new(),
    hideleg: ReadWriteRiscvCsr::new(),
    hie: ReadWriteRiscvCsr::new(),
    hcounteren: ReadWriteRiscvCsr::new(),
    hgeie: ReadWriteRiscvCsr::new(),
    hvictl: ReadWriteRiscvCsr::new(),
    htval: ReadWriteRiscvCsr::new(),
    hip: ReadWriteRiscvCsr::new(),
    hvip: ReadWriteRiscvCsr::new(),
    htinst: ReadWriteRiscvCsr::new(),
    hgeip: ReadWriteRiscvCsr::new(),
    henvcfg: ReadWriteRiscvCsr::new(),
    hstateen0: ReadWriteRiscvCsr::new(),
    hgatp: ReadWriteRiscvCsr::new(),
    htimedelta: ReadWriteRiscvCsr::new(),

    // Supervisor (S-mode) registers.
    sstatus: ReadWriteRiscvCsr::new(),
    sie: ReadWriteRiscvCsr::new(),
    stvec: ReadWriteRiscvCsr::new(),
    scounteren: ReadWriteRiscvCsr::new(),
    sscratch: ReadWriteRiscvCsr::new(),
    sepc: ReadWriteRiscvCsr::new(),
    scause: ReadWriteRiscvCsr::new(),
    stval: ReadWriteRiscvCsr::new(),
    sip: ReadWriteRiscvCsr::new(),
    stimecmp: ReadWriteRiscvCsr::new(),
    stopei: ReadWriteRiscvCsr::new(),
    satp: ReadWriteRiscvCsr::new(),
    stopi: ReadWriteRiscvCsr::new(),

    // VCoVE: seed CSR emulation support.
    seed: ReadWriteRiscvCsr::new(),

    // Virtual supervisor (VS-mode) registers.
    vsstatus: ReadWriteRiscvCsr::new(),
    vsie: ReadWriteRiscvCsr::new(),
    vstvec: ReadWriteRiscvCsr::new(),
    vsscratch: ReadWriteRiscvCsr::new(),
    vsepc: ReadWriteRiscvCsr::new(),
    vscause: ReadWriteRiscvCsr::new(),
    vstval: ReadWriteRiscvCsr::new(),
    vsip: ReadWriteRiscvCsr::new(),
    vstimecmp: ReadWriteRiscvCsr::new(),
    vstopei: ReadWriteRiscvCsr::new(),
    vsatp: ReadWriteRiscvCsr::new(),
    vstopi: ReadWriteRiscvCsr::new(),

    // Vector extension registers.
    vstart: ReadWriteRiscvCsr::new(),
    vcsr: ReadWriteRiscvCsr::new(),
    vl: ReadWriteRiscvCsr::new(),
    vtype: ReadWriteRiscvCsr::new(),
    vlenb: ReadWriteRiscvCsr::new(),

    // The 32 hpmcounter CSRs occupy the contiguous number range
    // 0xc00..=0xc1f; seq! stamps out one accessor per counter.
    hpmcounter: seq!(N in 0xc00..=0xc1f {[
        #( &ReadWriteRiscvCsr::<hpmcounter::Register, N>::new(), )*
    ]}),

    // S-mode IMSIC registers behind the siselect/sireg window.
    si_eidelivery: SiselectReg::new(SISELECT, SIREG),
    si_eithreshold: SiselectReg::new(SISELECT, SIREG),
    // Only even selector numbers are valid on RV64, hence the stride
    // of 2 applied to the seq! counter.
    si_eip: seq!(N in 0..=31 {[
        #( &SiselectReg::<eip::Register, {ISELECT_EIP_BASE + 2 * N}>::new(SISELECT, SIREG), )*
    ]}),
    si_eie: seq!(N in 0..=31 {[
        #( &SiselectReg::<eie::Register, {ISELECT_EIE_BASE + 2 * N}>::new(SISELECT, SIREG), )*
    ]}),

    // VS-mode IMSIC registers behind the vsiselect/vsireg window.
    vsi_eidelivery: VsiselectReg::new(VSISELECT, VSIREG),
    vsi_eithreshold: VsiselectReg::new(VSISELECT, VSIREG),
    // Same even-selector stride as the S-mode arrays above.
    vsi_eip: seq!(N in 0..=31 {[
        #( &VsiselectReg::<eip::Register, {ISELECT_EIP_BASE + 2 * N}>::new(VSISELECT, VSIREG), )*
    ]}),
    vsi_eie: seq!(N in 0..=31 {[
        #( &VsiselectReg::<eie::Register, {ISELECT_EIE_BASE + 2 * N}>::new(VSISELECT, VSIREG), )*
    ]}),
};
