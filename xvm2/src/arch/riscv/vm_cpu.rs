// SPDX-FileCopyrightText: 2026 DAMO Inc.
// SPDX-FileCopyrightText: 2023 Rivos Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! World-switch glue between Rust and the context-switch assembly.
//!
//! The byte layout of [`VmCpuRegisters`] is private to the Rust type
//! system, while `asm/guest.S` needs numeric offsets for every field it
//! touches. This module bridges the two: each offset is computed at
//! compile time (from `offset_of!` / `size_of`) and handed to
//! [`global_asm!`] as a named template parameter, so the struct layout
//! and the assembly can never drift apart silently.
//!
//! The register-context types themselves live in the standalone
//! `riscv-regs` crate so they can be built and examined on any host;
//! only this glue is RISC-V target specific.

use core::arch::global_asm;
use core::mem::size_of;
use memoffset::offset_of;
use riscv_regs::*;

// World-switch entry point and FP state helpers, defined in asm/guest.S.
extern "C" {
    fn _run_guest(state: *mut VmCpuRegisters);
    fn _save_fp(state: *mut VmCpuRegisters);
    fn _restore_fp(state: *mut VmCpuRegisters);
}

// Vector state helpers, defined in asm/guest.S.
extern "C" {
    fn _restore_vector(state: *mut VmCpuRegisters);
    fn _save_vector(state: *mut VmCpuRegisters);
}

/// Width in bytes of every slot in the GPR and FPR files.
const SLOT_WIDTH: usize = size_of::<u64>();

/// Byte offset of the slot at positional `index` in a register file
/// whose first element starts `file_base` bytes into [`VmCpuRegisters`].
const fn slot_from(file_base: usize, index: usize, stride: usize) -> usize {
    file_base + index * stride
}

/// Byte offset of hypervisor GPR `index` within [`VmCpuRegisters`].
#[allow(dead_code)]
const fn hyp_xreg_offset(index: GprIndex) -> usize {
    let file_base = offset_of!(VmCpuRegisters, hyp_regs) + offset_of!(HypervisorCpuState, gprs);
    slot_from(file_base, index as usize, SLOT_WIDTH)
}

/// Byte offset of guest GPR `index` within [`VmCpuRegisters`].
#[allow(dead_code)]
const fn guest_xreg_offset(index: GprIndex) -> usize {
    let file_base = offset_of!(VmCpuRegisters, guest_regs) + offset_of!(GuestCpuState, gprs);
    slot_from(file_base, index as usize, SLOT_WIDTH)
}

/// Byte offset of guest FP register `index` within [`VmCpuRegisters`].
const fn guest_freg_offset(index: usize) -> usize {
    let file_base = offset_of!(VmCpuRegisters, guest_regs) + offset_of!(GuestCpuState, fprs);
    slot_from(file_base, index, SLOT_WIDTH)
}

/// Byte offset of guest vector register `index` within [`VmCpuRegisters`].
const fn guest_vreg_offset(index: usize) -> usize {
    let file_base = offset_of!(VmCpuRegisters, guest_regs) + offset_of!(GuestCpuState, vprs);
    slot_from(file_base, index, size_of::<riscv_regs::VectorRegister>())
}

/// Byte offset of a CSR holdover field within [`VmCpuRegisters`]: `hyp`
/// reaches the hypervisor context in `hyp_regs`, `guest` the guest
/// context in `guest_regs`.
macro_rules! ctx_field_offset {
    (hyp, $field:tt) => {
        offset_of!(VmCpuRegisters, hyp_regs) + offset_of!(HypervisorCpuState, $field)
    };
    (guest, $field:tt) => {
        offset_of!(VmCpuRegisters, guest_regs) + offset_of!(GuestCpuState, $field)
    };
}

// The assembler binds each parameter by name, so the grouping and the
// ordering below are purely cosmetic; the groups mirror the phases of
// the world switch implemented in asm/guest.S.
global_asm!(
    include_str!("asm/guest.S"),
    // ---- Hypervisor GPR spill slots ----
    hyp_ra = const hyp_xreg_offset(GprIndex::RA),
    hyp_gp = const hyp_xreg_offset(GprIndex::GP),
    hyp_tp = const hyp_xreg_offset(GprIndex::TP),
    hyp_s0 = const hyp_xreg_offset(GprIndex::S0),
    hyp_s1 = const hyp_xreg_offset(GprIndex::S1),
    hyp_a1 = const hyp_xreg_offset(GprIndex::A1),
    hyp_a2 = const hyp_xreg_offset(GprIndex::A2),
    hyp_a3 = const hyp_xreg_offset(GprIndex::A3),
    hyp_a4 = const hyp_xreg_offset(GprIndex::A4),
    hyp_a5 = const hyp_xreg_offset(GprIndex::A5),
    hyp_a6 = const hyp_xreg_offset(GprIndex::A6),
    hyp_a7 = const hyp_xreg_offset(GprIndex::A7),
    hyp_s2 = const hyp_xreg_offset(GprIndex::S2),
    hyp_s3 = const hyp_xreg_offset(GprIndex::S3),
    hyp_s4 = const hyp_xreg_offset(GprIndex::S4),
    hyp_s5 = const hyp_xreg_offset(GprIndex::S5),
    hyp_s6 = const hyp_xreg_offset(GprIndex::S6),
    hyp_s7 = const hyp_xreg_offset(GprIndex::S7),
    hyp_s8 = const hyp_xreg_offset(GprIndex::S8),
    hyp_s9 = const hyp_xreg_offset(GprIndex::S9),
    hyp_s10 = const hyp_xreg_offset(GprIndex::S10),
    hyp_s11 = const hyp_xreg_offset(GprIndex::S11),
    hyp_sp = const hyp_xreg_offset(GprIndex::SP),
    // ---- Guest GPR slots ----
    guest_ra = const guest_xreg_offset(GprIndex::RA),
    guest_gp = const guest_xreg_offset(GprIndex::GP),
    guest_tp = const guest_xreg_offset(GprIndex::TP),
    guest_s0 = const guest_xreg_offset(GprIndex::S0),
    guest_s1 = const guest_xreg_offset(GprIndex::S1),
    guest_a0 = const guest_xreg_offset(GprIndex::A0),
    guest_a1 = const guest_xreg_offset(GprIndex::A1),
    guest_a2 = const guest_xreg_offset(GprIndex::A2),
    guest_a3 = const guest_xreg_offset(GprIndex::A3),
    guest_a4 = const guest_xreg_offset(GprIndex::A4),
    guest_a5 = const guest_xreg_offset(GprIndex::A5),
    guest_a6 = const guest_xreg_offset(GprIndex::A6),
    guest_a7 = const guest_xreg_offset(GprIndex::A7),
    guest_s2 = const guest_xreg_offset(GprIndex::S2),
    guest_s3 = const guest_xreg_offset(GprIndex::S3),
    guest_s4 = const guest_xreg_offset(GprIndex::S4),
    guest_s5 = const guest_xreg_offset(GprIndex::S5),
    guest_s6 = const guest_xreg_offset(GprIndex::S6),
    guest_s7 = const guest_xreg_offset(GprIndex::S7),
    guest_s8 = const guest_xreg_offset(GprIndex::S8),
    guest_s9 = const guest_xreg_offset(GprIndex::S9),
    guest_s10 = const guest_xreg_offset(GprIndex::S10),
    guest_s11 = const guest_xreg_offset(GprIndex::S11),
    guest_t0 = const guest_xreg_offset(GprIndex::T0),
    guest_t1 = const guest_xreg_offset(GprIndex::T1),
    guest_t2 = const guest_xreg_offset(GprIndex::T2),
    guest_t3 = const guest_xreg_offset(GprIndex::T3),
    guest_t4 = const guest_xreg_offset(GprIndex::T4),
    guest_t5 = const guest_xreg_offset(GprIndex::T5),
    guest_t6 = const guest_xreg_offset(GprIndex::T6),
    guest_sp = const guest_xreg_offset(GprIndex::SP),
    // ---- Hypervisor CSR holdovers ----
    hyp_sstatus = const ctx_field_offset!(hyp, sstatus),
    hyp_hstatus = const ctx_field_offset!(hyp, hstatus),
    hyp_scounteren = const ctx_field_offset!(hyp, scounteren),
    hyp_stvec = const ctx_field_offset!(hyp, stvec),
    hyp_sscratch = const ctx_field_offset!(hyp, sscratch),
    // ---- Guest CSR holdovers ----
    guest_sstatus = const ctx_field_offset!(guest, sstatus),
    guest_hstatus = const ctx_field_offset!(guest, hstatus),
    guest_scounteren = const ctx_field_offset!(guest, scounteren),
    guest_sepc = const ctx_field_offset!(guest, sepc),
    guest_fcsr = const ctx_field_offset!(guest, fcsr),
    guest_vstart = const ctx_field_offset!(guest, vstart),
    guest_vcsr = const ctx_field_offset!(guest, vcsr),
    guest_vtype = const ctx_field_offset!(guest, vtype),
    guest_vl = const ctx_field_offset!(guest, vl),
    // ---- Guest FPR slots ----
    // FP scratch registers ft0-ft7
    guest_f0 = const guest_freg_offset(0),
    guest_f1 = const guest_freg_offset(1),
    guest_f2 = const guest_freg_offset(2),
    guest_f3 = const guest_freg_offset(3),
    guest_f4 = const guest_freg_offset(4),
    guest_f5 = const guest_freg_offset(5),
    guest_f6 = const guest_freg_offset(6),
    guest_f7 = const guest_freg_offset(7),
    // FP saved registers fs0-fs1
    guest_f8 = const guest_freg_offset(8),
    guest_f9 = const guest_freg_offset(9),
    // FP argument/return-value registers fa0-fa7
    guest_f10 = const guest_freg_offset(10),
    guest_f11 = const guest_freg_offset(11),
    guest_f12 = const guest_freg_offset(12),
    guest_f13 = const guest_freg_offset(13),
    guest_f14 = const guest_freg_offset(14),
    guest_f15 = const guest_freg_offset(15),
    guest_f16 = const guest_freg_offset(16),
    guest_f17 = const guest_freg_offset(17),
    // further FP saved registers fs2-fs11
    guest_f18 = const guest_freg_offset(18),
    guest_f19 = const guest_freg_offset(19),
    guest_f20 = const guest_freg_offset(20),
    guest_f21 = const guest_freg_offset(21),
    guest_f22 = const guest_freg_offset(22),
    guest_f23 = const guest_freg_offset(23),
    guest_f24 = const guest_freg_offset(24),
    guest_f25 = const guest_freg_offset(25),
    guest_f26 = const guest_freg_offset(26),
    guest_f27 = const guest_freg_offset(27),
    // FP scratch registers ft8-ft11
    guest_f28 = const guest_freg_offset(28),
    guest_f29 = const guest_freg_offset(29),
    guest_f30 = const guest_freg_offset(30),
    guest_f31 = const guest_freg_offset(31),
    // ---- Guest VPR group bases (v0/v8/v16/v24, eight registers each) ----
    guest_v0 = const guest_vreg_offset(0),
    guest_v8 = const guest_vreg_offset(8),
    guest_v16 = const guest_vreg_offset(16),
    guest_v24 = const guest_vreg_offset(24),
    // ---- sstatus patterns used to enable FP/vector temporarily ----
    sstatus_fs_dirty = const sstatus::fs::Dirty.value,
    sstatus_vs_enable = const sstatus::vs::Initial.value,
);
