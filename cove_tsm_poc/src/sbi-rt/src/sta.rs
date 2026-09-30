// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! STA Extension (EID 0x535441, ASCII "STA").
//!
//! The Steal-time Accounting Extension provides supervisor-mode software
//! with steal-time and preemption information for virtual harts. A
//! shared memory region is used to communicate statistics between the
//! SBI implementation and the supervisor.
//!
//! | FID | Name      | Parameters                              |
//! |-----|-----------|-----------------------------------------|
//! |  0  | SetShmem  | shmem_phys_lo, shmem_phys_hi, flags     |
//!
//! Reference: RISC-V SBI Specification v3.0, Chapter 16.

use crate::function::SbiFunction;

const FID_SET_SHMEM: u64 = 0;

/// Functions defined by the STA Extension (EID 0x535441).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StaFunction {
    /// Set the shared memory region for steal-time data (FID #0).
    SetShmem {
        /// Physical address low bits.
        shmem_phys_lo: u64,
        /// Physical address high bits.
        shmem_phys_hi: u64,
        /// Flags (reserved, must be 0).
        flags: u64,
    },
}

impl StaFunction {
    /// Parses a `StaFunction` from the saved register state.
    pub fn from_regs(args: &[u64]) -> crate::error::Result<Self> {
        match args[6] {
            FID_SET_SHMEM => Ok(StaFunction::SetShmem {
                shmem_phys_lo: args[0],
                shmem_phys_hi: args[1],
                flags: args[2],
            }),
            _ => Err(crate::error::Error::NotSupported),
        }
    }
}

impl SbiFunction for StaFunction {
    fn a6(&self) -> u64 {
        match self {
            StaFunction::SetShmem { .. } => FID_SET_SHMEM,
        }
    }

    fn a0(&self) -> u64 {
        match self {
            StaFunction::SetShmem { shmem_phys_lo, .. } => *shmem_phys_lo,
        }
    }

    fn a1(&self) -> u64 {
        match self {
            StaFunction::SetShmem { shmem_phys_hi, .. } => *shmem_phys_hi,
        }
    }

    fn a2(&self) -> u64 {
        match self {
            StaFunction::SetShmem { flags, .. } => *flags,
        }
    }
}
