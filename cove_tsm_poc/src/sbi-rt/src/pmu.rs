// SPDX-FileCopyrightText: 2026 DAMO Inc.
// SPDX-FileCopyrightText: 2023 Rivos Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! PMU Extension (EID 0x504D55, ASCII "PMU").
//!
//! The Performance Monitoring Unit Extension provides supervisor-mode
//! software with functions to configure and use RISC-V hardware
//! performance counters.
//!
//! | FID | Name                    | Key parameters                          |
//! |-----|-------------------------|-----------------------------------------|
//! |  0  | NumCounters             | (none)                                  |
//! |  1  | CounterGetInfo          | counter_idx                             |
//! |  2  | CounterConfigMatching   | base, mask, flags, event_idx, event_data|
//! |  3  | CounterStart            | base, mask, flags, initial_value        |
//! |  4  | CounterStop             | base, mask, flags                       |
//! |  5  | CounterFwRead           | counter_idx                             |
//! |  6  | CounterFwReadHi         | counter_idx                             |
//! |  7  | SnapshotSetShmem        | shmem_lo, shmem_hi, flags               |
//! |  8  | EventGetInfo            | shmem_lo, shmem_hi, num_entries, flags   |
//!
//! Reference: RISC-V SBI Specification v3.0, Chapter 11.

use crate::function::SbiFunction;

const FID_NUM_COUNTERS: u64 = 0;
const FID_COUNTER_GET_INFO: u64 = 1;
const FID_COUNTER_CONFIG_MATCHING: u64 = 2;
const FID_COUNTER_START: u64 = 3;
const FID_COUNTER_STOP: u64 = 4;
const FID_COUNTER_FW_READ: u64 = 5;
const FID_COUNTER_FW_READ_HI: u64 = 6;
const FID_SNAPSHOT_SET_SHMEM: u64 = 7;
const FID_EVENT_GET_INFO: u64 = 8;

/// Functions defined by the PMU Extension (EID 0x504D55).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PmuFunction {
    /// Return the total number of counters (FID #0).
    NumCounters,
    /// Return information about a counter (FID #1).
    CounterGetInfo {
        /// Counter index to query.
        counter_idx: u64,
    },
    /// Find and configure a matching counter for an event (FID #2).
    CounterConfigMatching {
        /// Counter set base index.
        counter_idx_base: u64,
        /// Counter set mask.
        counter_idx_mask: u64,
        /// Configuration and filter flags.
        config_flags: u64,
        /// Event index (type[19:16] | code[15:0]).
        event_idx: u64,
        /// Additional event data.
        event_data: u64,
    },
    /// Start one or more counters (FID #3).
    CounterStart {
        /// Counter set base index.
        counter_idx_base: u64,
        /// Counter set mask.
        counter_idx_mask: u64,
        /// Start flags.
        start_flags: u64,
        /// Initial counter value.
        initial_value: u64,
    },
    /// Stop one or more counters (FID #4).
    CounterStop {
        /// Counter set base index.
        counter_idx_base: u64,
        /// Counter set mask.
        counter_idx_mask: u64,
        /// Stop flags.
        stop_flags: u64,
    },
    /// Read a firmware counter value (FID #5).
    CounterFwRead {
        /// Counter index.
        counter_idx: u64,
    },
    /// Read high 32 bits of a firmware counter (FID #6).
    CounterFwReadHi {
        /// Counter index.
        counter_idx: u64,
    },
    /// Set PMU snapshot shared memory (FID #7).
    SnapshotSetShmem {
        /// Shared memory physical address low bits.
        shmem_phys_lo: u64,
        /// Shared memory physical address high bits.
        shmem_phys_hi: u64,
        /// Flags (must be 0).
        flags: u64,
    },
    /// Get event information (FID #8).
    EventGetInfo {
        /// Shared memory physical address low bits.
        shmem_phys_lo: u64,
        /// Shared memory physical address high bits.
        shmem_phys_hi: u64,
        /// Number of event entries.
        num_entries: u64,
        /// Flags (must be 0).
        flags: u64,
    },
}

impl PmuFunction {
    /// Parses a `PmuFunction` from the saved register state.
    pub fn from_regs(args: &[u64]) -> crate::error::Result<Self> {
        match args[6] {
            FID_NUM_COUNTERS => Ok(PmuFunction::NumCounters),
            FID_COUNTER_GET_INFO => Ok(PmuFunction::CounterGetInfo {
                counter_idx: args[0],
            }),
            FID_COUNTER_CONFIG_MATCHING => Ok(PmuFunction::CounterConfigMatching {
                counter_idx_base: args[0],
                counter_idx_mask: args[1],
                config_flags: args[2],
                event_idx: args[3],
                event_data: args[4],
            }),
            FID_COUNTER_START => Ok(PmuFunction::CounterStart {
                counter_idx_base: args[0],
                counter_idx_mask: args[1],
                start_flags: args[2],
                initial_value: args[3],
            }),
            FID_COUNTER_STOP => Ok(PmuFunction::CounterStop {
                counter_idx_base: args[0],
                counter_idx_mask: args[1],
                stop_flags: args[2],
            }),
            FID_COUNTER_FW_READ => Ok(PmuFunction::CounterFwRead {
                counter_idx: args[0],
            }),
            FID_COUNTER_FW_READ_HI => Ok(PmuFunction::CounterFwReadHi {
                counter_idx: args[0],
            }),
            FID_SNAPSHOT_SET_SHMEM => Ok(PmuFunction::SnapshotSetShmem {
                shmem_phys_lo: args[0],
                shmem_phys_hi: args[1],
                flags: args[2],
            }),
            FID_EVENT_GET_INFO => Ok(PmuFunction::EventGetInfo {
                shmem_phys_lo: args[0],
                shmem_phys_hi: args[1],
                num_entries: args[2],
                flags: args[3],
            }),
            _ => Err(crate::error::Error::NotSupported),
        }
    }
}

impl SbiFunction for PmuFunction {
    fn a6(&self) -> u64 {
        match self {
            PmuFunction::NumCounters => FID_NUM_COUNTERS,
            PmuFunction::CounterGetInfo { .. } => FID_COUNTER_GET_INFO,
            PmuFunction::CounterConfigMatching { .. } => FID_COUNTER_CONFIG_MATCHING,
            PmuFunction::CounterStart { .. } => FID_COUNTER_START,
            PmuFunction::CounterStop { .. } => FID_COUNTER_STOP,
            PmuFunction::CounterFwRead { .. } => FID_COUNTER_FW_READ,
            PmuFunction::CounterFwReadHi { .. } => FID_COUNTER_FW_READ_HI,
            PmuFunction::SnapshotSetShmem { .. } => FID_SNAPSHOT_SET_SHMEM,
            PmuFunction::EventGetInfo { .. } => FID_EVENT_GET_INFO,
        }
    }

    fn a0(&self) -> u64 {
        match self {
            PmuFunction::NumCounters => 0,
            PmuFunction::CounterGetInfo { counter_idx } => *counter_idx,
            PmuFunction::CounterConfigMatching {
                counter_idx_base, ..
            } => *counter_idx_base,
            PmuFunction::CounterStart {
                counter_idx_base, ..
            } => *counter_idx_base,
            PmuFunction::CounterStop {
                counter_idx_base, ..
            } => *counter_idx_base,
            PmuFunction::CounterFwRead { counter_idx } => *counter_idx,
            PmuFunction::CounterFwReadHi { counter_idx } => *counter_idx,
            PmuFunction::SnapshotSetShmem { shmem_phys_lo, .. } => *shmem_phys_lo,
            PmuFunction::EventGetInfo { shmem_phys_lo, .. } => *shmem_phys_lo,
        }
    }

    fn a1(&self) -> u64 {
        match self {
            PmuFunction::CounterConfigMatching {
                counter_idx_mask, ..
            } => *counter_idx_mask,
            PmuFunction::CounterStart {
                counter_idx_mask, ..
            } => *counter_idx_mask,
            PmuFunction::CounterStop {
                counter_idx_mask, ..
            } => *counter_idx_mask,
            PmuFunction::SnapshotSetShmem { shmem_phys_hi, .. } => *shmem_phys_hi,
            PmuFunction::EventGetInfo { shmem_phys_hi, .. } => *shmem_phys_hi,
            _ => 0,
        }
    }

    fn a2(&self) -> u64 {
        match self {
            PmuFunction::CounterConfigMatching { config_flags, .. } => *config_flags,
            PmuFunction::CounterStart { start_flags, .. } => *start_flags,
            PmuFunction::CounterStop { stop_flags, .. } => *stop_flags,
            PmuFunction::SnapshotSetShmem { flags, .. } => *flags,
            PmuFunction::EventGetInfo { num_entries, .. } => *num_entries,
            _ => 0,
        }
    }

    fn a3(&self) -> u64 {
        match self {
            PmuFunction::CounterConfigMatching { event_idx, .. } => *event_idx,
            PmuFunction::CounterStart { initial_value, .. } => *initial_value,
            PmuFunction::EventGetInfo { flags, .. } => *flags,
            _ => 0,
        }
    }

    fn a4(&self) -> u64 {
        match self {
            PmuFunction::CounterConfigMatching { event_data, .. } => *event_data,
            _ => 0,
        }
    }
}
