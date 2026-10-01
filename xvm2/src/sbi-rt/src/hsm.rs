// SPDX-FileCopyrightText: 2026 DAMO Inc.
// SPDX-FileCopyrightText: 2023 Rivos Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! HSM Extension (EID 0x48534D, ASCII "HSM").
//!
//! The Hart State Management Extension provides functions for starting,
//! stopping, querying status of, and suspending harts.
//!
//! | FID | Name           | Parameters                                   |
//! |-----|----------------|----------------------------------------------|
//! |  0  | HartStart      | a0 = hartid, a1 = start_addr, a2 = opaque   |
//! |  1  | HartStop       | (none)                                       |
//! |  2  | HartGetStatus  | a0 = hartid                                  |
//! |  3  | HartSuspend    | a0 = suspend_type, a1 = resume_addr, a2 = op |
//!
//! Hart states returned by `HartGetStatus` (in a1):
//!   0 = STARTED, 1 = STOPPED, 2 = START_PENDING, 3 = STOP_PENDING,
//!   4 = SUSPENDED, 5 = SUSPEND_PENDING, 6 = RESUME_PENDING.
//!
//! Reference: RISC-V SBI Specification v3.0, Chapter 9.

use crate::function::SbiFunction;

const FID_HART_START: u64 = 0;
const FID_HART_STOP: u64 = 1;
const FID_HART_GET_STATUS: u64 = 2;
const FID_HART_SUSPEND: u64 = 3;

/// Functions defined by the HSM Extension (EID 0x48534D).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HsmFunction {
    /// Start a hart at the given address (FID #0).
    HartStart {
        /// Target hart ID to start.
        hartid: u64,
        /// Physical address where the hart begins execution.
        start_addr: u64,
        /// Opaque value passed in a1 to the started hart.
        opaque: u64,
    },
    /// Stop the calling hart (FID #1). This call does not return on success.
    HartStop,
    /// Query the state of a hart (FID #2).
    HartGetStatus {
        /// Target hart ID to query.
        hartid: u64,
    },
    /// Suspend the calling hart (FID #3).
    HartSuspend {
        /// Suspend type (retentive or non-retentive).
        suspend_type: u64,
        /// Physical address for resumption (non-retentive only).
        resume_addr: u64,
        /// Opaque value passed in a1 on resume (non-retentive only).
        opaque: u64,
    },
}

impl HsmFunction {
    /// Parses an `HsmFunction` from the saved register state.
    pub fn from_regs(args: &[u64]) -> crate::error::Result<Self> {
        match args[6] {
            FID_HART_START => Ok(HsmFunction::HartStart {
                hartid: args[0],
                start_addr: args[1],
                opaque: args[2],
            }),
            FID_HART_STOP => Ok(HsmFunction::HartStop),
            FID_HART_GET_STATUS => Ok(HsmFunction::HartGetStatus { hartid: args[0] }),
            FID_HART_SUSPEND => Ok(HsmFunction::HartSuspend {
                suspend_type: args[0],
                resume_addr: args[1],
                opaque: args[2],
            }),
            _ => Err(crate::error::Error::NotSupported),
        }
    }
}

impl SbiFunction for HsmFunction {
    fn a6(&self) -> u64 {
        match self {
            HsmFunction::HartStart { .. } => FID_HART_START,
            HsmFunction::HartStop => FID_HART_STOP,
            HsmFunction::HartGetStatus { .. } => FID_HART_GET_STATUS,
            HsmFunction::HartSuspend { .. } => FID_HART_SUSPEND,
        }
    }

    fn a0(&self) -> u64 {
        match self {
            HsmFunction::HartStart { hartid, .. } => *hartid,
            HsmFunction::HartStop => 0,
            HsmFunction::HartGetStatus { hartid } => *hartid,
            HsmFunction::HartSuspend { suspend_type, .. } => *suspend_type,
        }
    }

    fn a1(&self) -> u64 {
        match self {
            HsmFunction::HartStart { start_addr, .. } => *start_addr,
            HsmFunction::HartSuspend { resume_addr, .. } => *resume_addr,
            _ => 0,
        }
    }

    fn a2(&self) -> u64 {
        match self {
            HsmFunction::HartStart { opaque, .. } => *opaque,
            HsmFunction::HartSuspend { opaque, .. } => *opaque,
            _ => 0,
        }
    }
}
