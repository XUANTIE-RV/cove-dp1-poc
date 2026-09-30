// SPDX-FileCopyrightText: 2026 DAMO Inc.
// SPDX-FileCopyrightText: 2023 Rivos Inc.
//
// SPDX-License-Identifier: Apache-2.0

use crate::error::*;

/// A Trait for an SbiFunction. Implementers use this trait to specify how to parse from and
/// serialize into the a0-a7 registers used to make SBI calls.
///
/// All register accessors default to `0`, so an implementation only needs
/// to override the slots its function actually populates. Under the base
/// SBI calling convention the register layout is: `a7` = extension ID
/// (supplied by the `SbiMessage` dispatch rather than by this trait), `a6` =
/// function ID, and `a0`-`a5` = the remaining function arguments.
pub trait SbiFunction {
    /// Returns the `u64` value that should be stored in register a6 before making the ecall for
    /// this function. By convention this is the function ID (FID) within the
    /// extension.
    fn a6(&self) -> u64 {
        0
    }
    /// Returns the `u64` value that should be stored in register a5 before making the ecall for
    /// this function.
    fn a5(&self) -> u64 {
        0
    }
    /// Returns the `u64` value that should be stored in register a4 before making the ecall for
    /// this function.
    fn a4(&self) -> u64 {
        0
    }
    /// Returns the `u64` value that should be stored in register a3 before making the ecall for
    /// this function.
    fn a3(&self) -> u64 {
        0
    }
    /// Returns the `u64` value that should be stored in register a2 before making the ecall for
    /// this function.
    fn a2(&self) -> u64 {
        0
    }
    /// Returns the `u64` value that should be stored in register a1 before making the ecall for
    /// this function.
    fn a1(&self) -> u64 {
        0
    }
    /// Returns the `u64` value that should be stored in register a0 before making the ecall for
    /// this function.
    fn a0(&self) -> u64 {
        0
    }
    /// Returns a result parsed from the a0 and a1 return value registers.
    ///
    /// Per the SBI return convention `a0` holds the error code (`0` on
    /// success) and `a1` holds the function's return value, so a non-zero
    /// `a0` is translated into an [`Error`] while a successful call yields
    /// the raw `a1` payload.
    fn result(&self, a0: u64, a1: u64) -> Result<u64> {
        match a0 {
            0 => Ok(a1),
            e => Err(Error::from_code(e as i64)),
        }
    }
}
