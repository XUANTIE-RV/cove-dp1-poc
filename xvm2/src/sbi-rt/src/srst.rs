// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! SRST Extension (EID 0x53525354, ASCII "SRST").
//!
//! The System Reset Extension provides a single function to request
//! system-level shutdown, cold reboot, or warm reboot.
//!
//! | FID | Name         | Parameters                            |
//! |-----|--------------|---------------------------------------|
//! |  0  | SystemReset  | a0 = reset_type, a1 = reset_reason    |
//!
//! On success this call does **not** return.
//!
//! Reference: RISC-V SBI Specification v3.0, Chapter 10.

use crate::function::SbiFunction;

const FID_SYSTEM_RESET: u64 = 0;

/// Functions defined by the SRST Extension (EID 0x53525354).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SrstFunction {
    /// Request a system reset (FID #0). Does not return on success.
    SystemReset {
        /// Reset type: 0 = Shutdown, 1 = Cold reboot, 2 = Warm reboot.
        reset_type: u64,
        /// Reset reason: 0 = No reason, 1 = System failure.
        reset_reason: u64,
    },
}

impl SrstFunction {
    /// Parses a `SrstFunction` from the saved register state.
    pub fn from_regs(args: &[u64]) -> crate::error::Result<Self> {
        match args[6] {
            FID_SYSTEM_RESET => Ok(SrstFunction::SystemReset {
                reset_type: args[0],
                reset_reason: args[1],
            }),
            _ => Err(crate::error::Error::NotSupported),
        }
    }
}

impl SbiFunction for SrstFunction {
    fn a6(&self) -> u64 {
        match self {
            SrstFunction::SystemReset { .. } => FID_SYSTEM_RESET,
        }
    }

    fn a0(&self) -> u64 {
        match self {
            SrstFunction::SystemReset { reset_type, .. } => *reset_type,
        }
    }

    fn a1(&self) -> u64 {
        match self {
            SrstFunction::SystemReset { reset_reason, .. } => *reset_reason,
        }
    }
}
