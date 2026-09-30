// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! IPI Extension (EID 0x735049, ASCII "sPI").
//!
//! The IPI Extension replaces the legacy Send IPI function (EID 0x04)
//! with a modern calling convention. It sends an inter-processor interrupt
//! to all harts specified in the hart mask.
//!
//! This extension has a single function:
//!
//! | FID | Name     | Parameters                               |
//! |-----|----------|------------------------------------------|
//! |  0  | SendIpi  | a0 = hart_mask, a1 = hart_mask_base      |
//!
//! The `hart_mask` is a scalar bit-vector where bit *i* (from LSB)
//! corresponds to hartid `hart_mask_base + i`. Setting `hart_mask_base`
//! to `-1` (all ones) means "all available harts" and `hart_mask` is
//! ignored.
//!
//! On success, returns `SBI_SUCCESS` (0) in a0.
//!
//! Reference: RISC-V SBI Specification v3.0, Chapter 7.

use crate::function::SbiFunction;

/// FID for `sbi_send_ipi`.
const FID_SEND_IPI: u64 = 0;

/// Functions defined by the IPI Extension (EID 0x735049).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IpiFunction {
    /// Send an inter-processor interrupt (FID #0).
    ///
    /// Delivers a software interrupt to every hart whose bit is set in
    /// the mask constructed from `hart_mask` and `hart_mask_base`.
    SendIpi {
        /// Scalar bit-vector of target hartids relative to `hart_mask_base`.
        hart_mask: u64,
        /// Starting hartid for the bit-vector; `u64::MAX` means all harts.
        hart_mask_base: u64,
    },
}

impl IpiFunction {
    /// Parses an `IpiFunction` from the saved register state.
    pub fn from_regs(args: &[u64]) -> crate::error::Result<Self> {
        match args[6] {
            FID_SEND_IPI => Ok(IpiFunction::SendIpi {
                hart_mask: args[0],
                hart_mask_base: args[1],
            }),
            _ => Err(crate::error::Error::NotSupported),
        }
    }
}

impl SbiFunction for IpiFunction {
    fn a6(&self) -> u64 {
        match self {
            IpiFunction::SendIpi { .. } => FID_SEND_IPI,
        }
    }

    fn a0(&self) -> u64 {
        match self {
            IpiFunction::SendIpi { hart_mask, .. } => *hart_mask,
        }
    }

    fn a1(&self) -> u64 {
        match self {
            IpiFunction::SendIpi { hart_mask_base, .. } => *hart_mask_base,
        }
    }
}
