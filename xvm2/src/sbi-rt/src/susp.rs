// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! SUSP Extension (EID 0x53555350, ASCII "SUSP").
//!
//! The System Suspend Extension provides a single function to request
//! the system to transition into a specified sleep state.
//!
//! | FID | Name           | Parameters                                   |
//! |-----|----------------|----------------------------------------------|
//! |  0  | SystemSuspend  | a0 = sleep_type, a1 = resume_addr, a2 = opaque |
//!
//! On success this call does **not** return. The hart resumes execution
//! at `resume_addr` in supervisor-mode with MMU disabled, a0 = hartid,
//! a1 = opaque.
//!
//! Reference: RISC-V SBI Specification v3.0, Chapter 13.

use crate::function::SbiFunction;

const FID_SYSTEM_SUSPEND: u64 = 0;

/// Functions defined by the SUSP Extension (EID 0x53555350).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SuspFunction {
    /// Request a system suspend (FID #0). Does not return on success.
    SystemSuspend {
        /// Sleep type: 0 = SUSPEND_TO_RAM, 0x80000000+ = platform-specific.
        sleep_type: u64,
        /// Physical address to resume execution at after wakeup.
        resume_addr: u64,
        /// Opaque value passed back in a1 on resume.
        opaque: u64,
    },
}

impl SuspFunction {
    /// Parses a `SuspFunction` from the saved register state.
    pub fn from_regs(args: &[u64]) -> crate::error::Result<Self> {
        match args[6] {
            FID_SYSTEM_SUSPEND => Ok(SuspFunction::SystemSuspend {
                sleep_type: args[0],
                resume_addr: args[1],
                opaque: args[2],
            }),
            _ => Err(crate::error::Error::NotSupported),
        }
    }
}

impl SbiFunction for SuspFunction {
    fn a6(&self) -> u64 {
        match self {
            SuspFunction::SystemSuspend { .. } => FID_SYSTEM_SUSPEND,
        }
    }

    fn a0(&self) -> u64 {
        match self {
            SuspFunction::SystemSuspend { sleep_type, .. } => *sleep_type,
        }
    }

    fn a1(&self) -> u64 {
        match self {
            SuspFunction::SystemSuspend { resume_addr, .. } => *resume_addr,
        }
    }

    fn a2(&self) -> u64 {
        match self {
            SuspFunction::SystemSuspend { opaque, .. } => *opaque,
        }
    }
}
