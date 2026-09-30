// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! FWFT Extension (EID 0x46574654, ASCII "FWFT").
//!
//! The Firmware Features Extension provides supervisor-mode software
//! with a mechanism to query and configure firmware feature values.
//!
//! | FID | Name | Parameters                          |
//! |-----|------|-------------------------------------|
//! |  0  | Set  | feature_id, value, flags            |
//! |  1  | Get  | feature_id                          |
//!
//! Reference: RISC-V SBI Specification v3.0, Chapter 18.

use crate::function::SbiFunction;

const FID_FWFT_SET: u64 = 0;
const FID_FWFT_GET: u64 = 1;

/// Functions defined by the FWFT Extension (EID 0x46574654).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FwftFunction {
    /// Set a firmware feature value (FID #0).
    Set {
        /// 32-bit firmware feature identifier.
        feature_id: u64,
        /// Value to set for the feature.
        value: u64,
        /// Flags (bit 0 = LOCK).
        flags: u64,
    },
    /// Get the current value of a firmware feature (FID #1).
    Get {
        /// 32-bit firmware feature identifier.
        feature_id: u64,
    },
}

impl FwftFunction {
    /// Parses a `FwftFunction` from the saved register state.
    pub fn from_regs(args: &[u64]) -> crate::error::Result<Self> {
        match args[6] {
            FID_FWFT_SET => Ok(FwftFunction::Set {
                feature_id: args[0],
                value: args[1],
                flags: args[2],
            }),
            FID_FWFT_GET => Ok(FwftFunction::Get {
                feature_id: args[0],
            }),
            _ => Err(crate::error::Error::NotSupported),
        }
    }
}

impl SbiFunction for FwftFunction {
    fn a6(&self) -> u64 {
        match self {
            FwftFunction::Set { .. } => FID_FWFT_SET,
            FwftFunction::Get { .. } => FID_FWFT_GET,
        }
    }

    fn a0(&self) -> u64 {
        match self {
            FwftFunction::Set { feature_id, .. } => *feature_id,
            FwftFunction::Get { feature_id } => *feature_id,
        }
    }

    fn a1(&self) -> u64 {
        match self {
            FwftFunction::Set { value, .. } => *value,
            _ => 0,
        }
    }

    fn a2(&self) -> u64 {
        match self {
            FwftFunction::Set { flags, .. } => *flags,
            _ => 0,
        }
    }
}
