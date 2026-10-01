// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! CPPC Extension (EID 0x43505043, ASCII "CPPC").
//!
//! The Collaborative Processor Performance Control Extension provides
//! supervisor-mode software with an abstraction to access CPPC registers
//! through SBI calls.
//!
//! | FID | Name      | Parameters                       |
//! |-----|-----------|----------------------------------|
//! |  0  | Probe     | cppc_reg_id                      |
//! |  1  | Read      | cppc_reg_id                      |
//! |  2  | ReadHi    | cppc_reg_id                      |
//! |  3  | Write     | cppc_reg_id, val                 |
//!
//! Reference: RISC-V SBI Specification v3.0, Chapter 14.

use crate::function::SbiFunction;

const FID_CPPC_PROBE: u64 = 0;
const FID_CPPC_READ: u64 = 1;
const FID_CPPC_READ_HI: u64 = 2;
const FID_CPPC_WRITE: u64 = 3;

/// Functions defined by the CPPC Extension (EID 0x43505043).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CppcFunction {
    /// Probe whether a CPPC register is implemented (FID #0).
    /// Returns register width in sbiret.value, or 0 if not implemented.
    Probe {
        /// 32-bit CPPC register identifier.
        cppc_reg_id: u64,
    },
    /// Read a CPPC register value (FID #1).
    Read {
        /// 32-bit CPPC register identifier.
        cppc_reg_id: u64,
    },
    /// Read the high 32 bits of a CPPC register (FID #2).
    /// For 64-bit XLEN, always returns 0.
    ReadHi {
        /// 32-bit CPPC register identifier.
        cppc_reg_id: u64,
    },
    /// Write a value to a CPPC register (FID #3).
    Write {
        /// 32-bit CPPC register identifier.
        cppc_reg_id: u64,
        /// Value to write (64-bit).
        val: u64,
    },
}

impl CppcFunction {
    /// Parses a `CppcFunction` from the saved register state.
    pub fn from_regs(args: &[u64]) -> crate::error::Result<Self> {
        match args[6] {
            FID_CPPC_PROBE => Ok(CppcFunction::Probe {
                cppc_reg_id: args[0],
            }),
            FID_CPPC_READ => Ok(CppcFunction::Read {
                cppc_reg_id: args[0],
            }),
            FID_CPPC_READ_HI => Ok(CppcFunction::ReadHi {
                cppc_reg_id: args[0],
            }),
            FID_CPPC_WRITE => Ok(CppcFunction::Write {
                cppc_reg_id: args[0],
                val: args[1],
            }),
            _ => Err(crate::error::Error::NotSupported),
        }
    }
}

impl SbiFunction for CppcFunction {
    fn a6(&self) -> u64 {
        match self {
            CppcFunction::Probe { .. } => FID_CPPC_PROBE,
            CppcFunction::Read { .. } => FID_CPPC_READ,
            CppcFunction::ReadHi { .. } => FID_CPPC_READ_HI,
            CppcFunction::Write { .. } => FID_CPPC_WRITE,
        }
    }

    fn a0(&self) -> u64 {
        match self {
            CppcFunction::Probe { cppc_reg_id }
            | CppcFunction::Read { cppc_reg_id }
            | CppcFunction::ReadHi { cppc_reg_id } => *cppc_reg_id,
            CppcFunction::Write { cppc_reg_id, .. } => *cppc_reg_id,
        }
    }

    fn a1(&self) -> u64 {
        match self {
            CppcFunction::Write { val, .. } => *val,
            _ => 0,
        }
    }
}
