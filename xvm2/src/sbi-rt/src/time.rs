// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! Timer Extension (EID 0x54494D45, ASCII "TIME").
//!
//! The Timer Extension replaces the legacy Set Timer function (EID 0x00)
//! with a modern calling convention. It programs the next timer interrupt
//! at an absolute `stime_value` (the value of the `time` CSR at which the
//! supervisor timer interrupt should fire).
//!
//! This extension has a single function:
//!
//! | FID | Name      | Parameters             |
//! |-----|-----------|------------------------|
//! |  0  | SetTimer  | a0 = stime_value (u64) |
//!
//! On success, returns `SBI_SUCCESS` (0) in a0, with a1 = 0.
//!
//! Reference: RISC-V SBI Specification v3.0, Chapter 6.

use crate::function::SbiFunction;

/// FID for `sbi_set_timer`.
const FID_SET_TIMER: u64 = 0;

/// Functions defined by the Timer Extension (EID 0x54494D45).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeFunction {
    /// Schedule the next timer interrupt (FID #0).
    ///
    /// Clears any pending timer interrupt and programs the timer to fire
    /// when `time >= stime_value`. If `stime_value` is `u64::MAX`, the
    /// timer interrupt is effectively disabled.
    SetTimer {
        /// Absolute time value for the next timer interrupt.
        stime_value: u64,
    },
}

impl TimeFunction {
    /// Parses a `TimeFunction` from the saved register state.
    pub fn from_regs(args: &[u64]) -> crate::error::Result<Self> {
        match args[6] {
            FID_SET_TIMER => Ok(TimeFunction::SetTimer {
                stime_value: args[0],
            }),
            _ => Err(crate::error::Error::NotSupported),
        }
    }
}

impl SbiFunction for TimeFunction {
    fn a6(&self) -> u64 {
        match self {
            TimeFunction::SetTimer { .. } => FID_SET_TIMER,
        }
    }

    fn a0(&self) -> u64 {
        match self {
            TimeFunction::SetTimer { stime_value } => *stime_value,
        }
    }
}
