// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! DBTR Extension (EID 0x44425452, ASCII "DBTR").
//!
//! The Debug Triggers Extension provides supervisor-mode software with
//! an interface to manage hardware debug triggers (breakpoints,
//! watchpoints, instruction counters) via SBI calls, abstracting the
//! underlying Sdtrig hardware.
//!
//! | FID | Name               | Parameters                              |
//! |-----|--------------------|-----------------------------------------|
//! |  0  | NumTriggers        | trig_tdata1                             |
//! |  1  | SetShmem           | shmem_phys_lo, shmem_phys_hi, flags     |
//! |  2  | ReadTriggers       | trig_idx_base, trig_count               |
//! |  3  | InstallTriggers    | trig_count                              |
//! |  4  | UpdateTriggers     | trig_count                              |
//! |  5  | UninstallTriggers  | trig_idx_base, trig_idx_mask            |
//! |  6  | EnableTriggers     | trig_idx_base, trig_idx_mask            |
//! |  7  | DisableTriggers    | trig_idx_base, trig_idx_mask            |
//!
//! Reference: RISC-V SBI Specification v3.0, Chapter 19.

use crate::function::SbiFunction;

const FID_NUM_TRIGGERS: u64 = 0;
const FID_SET_SHMEM: u64 = 1;
const FID_READ_TRIGGERS: u64 = 2;
const FID_INSTALL_TRIGGERS: u64 = 3;
const FID_UPDATE_TRIGGERS: u64 = 4;
const FID_UNINSTALL_TRIGGERS: u64 = 5;
const FID_ENABLE_TRIGGERS: u64 = 6;
const FID_DISABLE_TRIGGERS: u64 = 7;

/// Functions defined by the DBTR Extension (EID 0x44425452).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DbtrFunction {
    /// Query the number of available triggers matching a type (FID #0).
    /// When trig_tdata1 is 0, returns the maximum number of triggers.
    NumTriggers {
        /// Trigger type filter encoded in tdata1 format, or 0 for all.
        trig_tdata1: u64,
    },
    /// Set the shared memory region for trigger data exchange (FID #1).
    SetShmem {
        /// Physical address low bits (must be aligned).
        shmem_phys_lo: u64,
        /// Physical address high bits.
        shmem_phys_hi: u64,
        /// Flags (reserved, must be 0).
        flags: u64,
    },
    /// Read trigger configurations into shared memory (FID #2).
    ReadTriggers {
        /// Base trigger index.
        trig_idx_base: u64,
        /// Number of triggers to read.
        trig_count: u64,
    },
    /// Install triggers from shared memory configuration (FID #3).
    InstallTriggers {
        /// Number of triggers to install (data in shared memory).
        trig_count: u64,
    },
    /// Update existing triggers from shared memory (FID #4).
    UpdateTriggers {
        /// Number of triggers to update (data in shared memory).
        trig_count: u64,
    },
    /// Uninstall triggers by index mask (FID #5).
    UninstallTriggers {
        /// Base trigger index.
        trig_idx_base: u64,
        /// Bitmask of triggers to uninstall relative to base.
        trig_idx_mask: u64,
    },
    /// Enable triggers by index mask (FID #6).
    EnableTriggers {
        /// Base trigger index.
        trig_idx_base: u64,
        /// Bitmask of triggers to enable relative to base.
        trig_idx_mask: u64,
    },
    /// Disable triggers by index mask (FID #7).
    DisableTriggers {
        /// Base trigger index.
        trig_idx_base: u64,
        /// Bitmask of triggers to disable relative to base.
        trig_idx_mask: u64,
    },
}

impl DbtrFunction {
    /// Parses a `DbtrFunction` from the saved register state.
    pub fn from_regs(args: &[u64]) -> crate::error::Result<Self> {
        match args[6] {
            FID_NUM_TRIGGERS => Ok(DbtrFunction::NumTriggers {
                trig_tdata1: args[0],
            }),
            FID_SET_SHMEM => Ok(DbtrFunction::SetShmem {
                shmem_phys_lo: args[0],
                shmem_phys_hi: args[1],
                flags: args[2],
            }),
            FID_READ_TRIGGERS => Ok(DbtrFunction::ReadTriggers {
                trig_idx_base: args[0],
                trig_count: args[1],
            }),
            FID_INSTALL_TRIGGERS => Ok(DbtrFunction::InstallTriggers {
                trig_count: args[0],
            }),
            FID_UPDATE_TRIGGERS => Ok(DbtrFunction::UpdateTriggers {
                trig_count: args[0],
            }),
            FID_UNINSTALL_TRIGGERS => Ok(DbtrFunction::UninstallTriggers {
                trig_idx_base: args[0],
                trig_idx_mask: args[1],
            }),
            FID_ENABLE_TRIGGERS => Ok(DbtrFunction::EnableTriggers {
                trig_idx_base: args[0],
                trig_idx_mask: args[1],
            }),
            FID_DISABLE_TRIGGERS => Ok(DbtrFunction::DisableTriggers {
                trig_idx_base: args[0],
                trig_idx_mask: args[1],
            }),
            _ => Err(crate::error::Error::NotSupported),
        }
    }
}

impl SbiFunction for DbtrFunction {
    fn a6(&self) -> u64 {
        match self {
            DbtrFunction::NumTriggers { .. } => FID_NUM_TRIGGERS,
            DbtrFunction::SetShmem { .. } => FID_SET_SHMEM,
            DbtrFunction::ReadTriggers { .. } => FID_READ_TRIGGERS,
            DbtrFunction::InstallTriggers { .. } => FID_INSTALL_TRIGGERS,
            DbtrFunction::UpdateTriggers { .. } => FID_UPDATE_TRIGGERS,
            DbtrFunction::UninstallTriggers { .. } => FID_UNINSTALL_TRIGGERS,
            DbtrFunction::EnableTriggers { .. } => FID_ENABLE_TRIGGERS,
            DbtrFunction::DisableTriggers { .. } => FID_DISABLE_TRIGGERS,
        }
    }

    fn a0(&self) -> u64 {
        match self {
            DbtrFunction::NumTriggers { trig_tdata1 } => *trig_tdata1,
            DbtrFunction::SetShmem { shmem_phys_lo, .. } => *shmem_phys_lo,
            DbtrFunction::ReadTriggers { trig_idx_base, .. } => *trig_idx_base,
            DbtrFunction::InstallTriggers { trig_count } => *trig_count,
            DbtrFunction::UpdateTriggers { trig_count } => *trig_count,
            DbtrFunction::UninstallTriggers { trig_idx_base, .. } => *trig_idx_base,
            DbtrFunction::EnableTriggers { trig_idx_base, .. } => *trig_idx_base,
            DbtrFunction::DisableTriggers { trig_idx_base, .. } => *trig_idx_base,
        }
    }

    fn a1(&self) -> u64 {
        match self {
            DbtrFunction::SetShmem { shmem_phys_hi, .. } => *shmem_phys_hi,
            DbtrFunction::ReadTriggers { trig_count, .. } => *trig_count,
            DbtrFunction::UninstallTriggers { trig_idx_mask, .. } => *trig_idx_mask,
            DbtrFunction::EnableTriggers { trig_idx_mask, .. } => *trig_idx_mask,
            DbtrFunction::DisableTriggers { trig_idx_mask, .. } => *trig_idx_mask,
            _ => 0,
        }
    }

    fn a2(&self) -> u64 {
        match self {
            DbtrFunction::SetShmem { flags, .. } => *flags,
            _ => 0,
        }
    }
}
