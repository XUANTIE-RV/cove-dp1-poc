// SPDX-FileCopyrightText: 2026 DAMO Inc.
// SPDX-FileCopyrightText: 2023 Rivos Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! NACL Extension (EID 0x4E41434C, ASCII "NACL").
//!
//! The Nested Acceleration Extension provides an interface for
//! supervisor-mode software (L1 hypervisor) to collaborate with the
//! SBI implementation (L0 hypervisor / TSM) through shared memory,
//! enabling batched CSR synchronization, HFENCE operations, and
//! optimized SRET paths.
//!
//! In CoVE, NACL is used for TSM ↔ Host communication (guest exit
//! info, MMIO results) rather than traditional nested virtualization.
//!
//! | FID | Name          | Parameters                              |
//! |-----|---------------|-----------------------------------------|
//! |  0  | ProbeFeature  | feature_id                              |
//! |  1  | SetShmem      | shmem_phys_lo, shmem_phys_hi, flags     |
//! |  2  | SyncCsr       | csr_num                                 |
//! |  3  | SyncHfence    | entry_index                             |
//! |  4  | SyncSret      | (none)                                  |
//!
//! Reference: RISC-V SBI Specification v3.0, Chapter 15.

use crate::function::SbiFunction;

const FID_PROBE_FEATURE: u64 = 0;
const FID_SET_SHMEM: u64 = 1;
const FID_SYNC_CSR: u64 = 2;
const FID_SYNC_HFENCE: u64 = 3;
const FID_SYNC_SRET: u64 = 4;

/// Functions defined by the NACL Extension (EID 0x4E41434C).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NaclFunction {
    /// Probe a specific NACL feature (FID #0).
    ProbeFeature {
        /// Feature identifier to probe.
        feature_id: u64,
    },
    /// Set the shared memory region for NACL operations (FID #1).
    SetShmem {
        /// Physical address low bits (must be 4K-aligned).
        shmem_phys_lo: u64,
        /// Physical address high bits.
        shmem_phys_hi: u64,
        /// Flags (reserved, must be 0).
        flags: u64,
    },
    /// Synchronize a single CSR value from shared memory (FID #2).
    SyncCsr {
        /// CSR number to synchronize.
        csr_num: u64,
    },
    /// Synchronize an HFENCE entry from shared memory (FID #3).
    SyncHfence {
        /// HFENCE entry index in shared memory.
        entry_index: u64,
    },
    /// Synchronize and perform SRET using shared memory state (FID #4).
    SyncSret,
}

impl NaclFunction {
    /// Parses a `NaclFunction` from the saved register state.
    pub fn from_regs(args: &[u64]) -> crate::error::Result<Self> {
        match args[6] {
            FID_PROBE_FEATURE => Ok(NaclFunction::ProbeFeature {
                feature_id: args[0],
            }),
            FID_SET_SHMEM => Ok(NaclFunction::SetShmem {
                shmem_phys_lo: args[0],
                shmem_phys_hi: args[1],
                flags: args[2],
            }),
            FID_SYNC_CSR => Ok(NaclFunction::SyncCsr { csr_num: args[0] }),
            FID_SYNC_HFENCE => Ok(NaclFunction::SyncHfence {
                entry_index: args[0],
            }),
            FID_SYNC_SRET => Ok(NaclFunction::SyncSret),
            _ => Err(crate::error::Error::NotSupported),
        }
    }
}

impl SbiFunction for NaclFunction {
    fn a6(&self) -> u64 {
        match self {
            NaclFunction::ProbeFeature { .. } => FID_PROBE_FEATURE,
            NaclFunction::SetShmem { .. } => FID_SET_SHMEM,
            NaclFunction::SyncCsr { .. } => FID_SYNC_CSR,
            NaclFunction::SyncHfence { .. } => FID_SYNC_HFENCE,
            NaclFunction::SyncSret => FID_SYNC_SRET,
        }
    }

    fn a0(&self) -> u64 {
        match self {
            NaclFunction::ProbeFeature { feature_id } => *feature_id,
            NaclFunction::SetShmem { shmem_phys_lo, .. } => *shmem_phys_lo,
            NaclFunction::SyncCsr { csr_num } => *csr_num,
            NaclFunction::SyncHfence { entry_index } => *entry_index,
            NaclFunction::SyncSret => 0,
        }
    }

    fn a1(&self) -> u64 {
        match self {
            NaclFunction::SetShmem { shmem_phys_hi, .. } => *shmem_phys_hi,
            _ => 0,
        }
    }

    fn a2(&self) -> u64 {
        match self {
            NaclFunction::SetShmem { flags, .. } => *flags,
            _ => 0,
        }
    }
}
