// SPDX-FileCopyrightText: 2026 DAMO Inc.
// SPDX-FileCopyrightText: 2023 Rivos Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! RV64 register-file abstractions.
//!
//! This module models the three architectural register files of an RV64
//! hart (integer, floating-point and vector) as plain `#[repr(C)]` arrays
//! of 64-bit words. The flat layout gives two guarantees that the
//! context-switch path relies on:
//!
//! * Zero-copy save/restore: assembly stubs compute field offsets with
//!   `offset_of!` and store/load registers directly into these arrays,
//!   so no marshalling code runs on the hot path.
//! * Stable ABI: field order and element width are frozen; any change
//!   here would silently corrupt the offsets baked into the assembly.

/// The RV64 integer register file (x0..x31), stored as a flat array of
/// 64-bit words so that assembly can address each slot by constant offset.
///
/// Holds the snapshot of a guest hart's integer state while the guest is
/// descheduled. `#[repr(C)]` is mandatory: the layout is consumed from
/// assembly via `offset_of!`-derived constants.
#[derive(Default)]
#[repr(C)]
pub struct GeneralPurposeRegisters([u64; 32]);

impl GeneralPurposeRegisters {
    /// Reads the 64-bit value currently held in the selected register slot.
    pub fn reg(&self, which: GprIndex) -> u64 {
        self.0[which as usize]
    }

    /// Writes `value` into the selected register slot.
    ///
    /// Writes targeting `x0` are silently dropped, mirroring the hardware
    /// semantics of the architecturally hard-wired zero register.
    pub fn set_reg(&mut self, which: GprIndex, value: u64) {
        if which == GprIndex::Zero {
            return;
        }

        self.0[which as usize] = value;
    }

    /// Returns the argument register window (`a0`..`a7`) as one slice.
    ///
    /// SBI dispatch reads every argument register anyway; exposing the
    /// contiguous window avoids eight individual accessor calls.
    pub fn a_regs(&self) -> &[u64] {
        &self.0[GprIndex::A0 as usize..=GprIndex::A7 as usize]
    }

    /// Returns the argument register window (`a0`..`a7`) as a mutable slice.
    pub fn a_regs_mut(&mut self) -> &mut [u64] {
        &mut self.0[GprIndex::A0 as usize..=GprIndex::A7 as usize]
    }
}

/// Symbolic index of each RV64 integer register inside
/// [`GeneralPurposeRegisters`].
///
/// Discriminants follow the architectural x-register encoding (x0..x31),
/// so a value of this enum can be cast directly to an array index or
/// compared against an instruction's rs/rd field.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GprIndex {
    Zero = 0,
    RA,
    SP,
    GP,
    TP,
    T0,
    T1,
    T2,
    S0,
    S1,
    A0,
    A1,
    A2,
    A3,
    A4,
    A5,
    A6,
    A7,
    S2,
    S3,
    S4,
    S5,
    S6,
    S7,
    S8,
    S9,
    S10,
    S11,
    T3,
    T4,
    T5,
    T6,
}

impl GprIndex {
    /// Decodes an architectural x-register number into a [`GprIndex`].
    ///
    /// Returns `None` when `encoding` falls outside the valid 0..=31
    /// range. Arms below are grouped by register class (special / caller-
    /// saved temporaries / callee-saved / arguments) purely for
    /// readability; every arm carries an explicit pattern, so the mapping
    /// is identical regardless of ordering.
    pub fn from_raw(encoding: u32) -> Option<Self> {
        use GprIndex::*;
        let decoded = match encoding {
            // Special-purpose registers.
            0 => Zero,
            1 => RA,
            2 => SP,
            3 => GP,
            4 => TP,
            // Caller-saved temporaries t0-t6.
            5 => T0,
            6 => T1,
            7 => T2,
            28 => T3,
            29 => T4,
            30 => T5,
            31 => T6,
            // Callee-saved registers s0-s11.
            8 => S0,
            9 => S1,
            18 => S2,
            19 => S3,
            20 => S4,
            21 => S5,
            22 => S6,
            23 => S7,
            24 => S8,
            25 => S9,
            26 => S10,
            27 => S11,
            // Argument registers a0-a7.
            10 => A0,
            11 => A1,
            12 => A2,
            13 => A3,
            14 => A4,
            15 => A5,
            16 => A6,
            17 => A7,
            _ => {
                return None;
            }
        };
        Some(decoded)
    }
}

/// The double-precision floating-point register file (f0..f31).
///
/// The hypervisor never interprets a guest's FP state; it only spills and
/// refills it wholesale, so an opaque array of 64-bit words is sufficient.
/// `#[repr(C)]` keeps the layout addressable from the assembly stubs.
#[derive(Default)]
#[repr(C)]
pub struct FloatingPointRegisters([u64; 32]);

/// Width of a single vector register in bytes, as assumed by this layout.
pub const MAX_VECTOR_REGISTER_LEN: usize = 32;
const U64S_IN_REGISTER: usize = MAX_VECTOR_REGISTER_LEN >> 3;

/// Backing storage for one vector register, split into 64-bit lanes.
#[derive(Default)]
#[repr(C)]
pub struct VectorRegister([u64; U64S_IN_REGISTER]);

/// The vector register file (v0..v31).
///
/// As with floating point, guest vector state is treated as opaque bulk
/// data during save/restore. Each entry is sized for a 256-bit register;
/// the true width is governed by the `vlenb` CSR, so this constant must
/// grow if hardware with `vlenb > 32` is ever targeted.
#[derive(Default)]
#[repr(C)]
pub struct VectorRegisters([VectorRegister; 32]);
