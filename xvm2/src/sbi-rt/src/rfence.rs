// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! RFENCE Extension (EID 0x52464E43, ASCII "RFNC").
//!
//! The RFENCE Extension replaces the legacy Remote FENCE.I (EID 0x05),
//! Remote SFENCE.VMA (EID 0x06), and Remote SFENCE.VMA with ASID (EID 0x07)
//! with modern calling conventions plus additional hypervisor fence functions.
//!
//! | FID | Name                      | Parameters                                           |
//! |-----|---------------------------|------------------------------------------------------|
//! |  0  | RemoteFenceI              | a0 = hart_mask, a1 = hart_mask_base                  |
//! |  1  | RemoteSfenceVma           | a0-a1 = hart_mask, a2 = start_addr, a3 = size        |
//! |  2  | RemoteSfenceVmaAsid       | a0-a1 = hart_mask, a2 = start_addr, a3 = size, a4=id |
//! |  3  | RemoteHfenceGvmaVmid      | a0-a1 = hart_mask, a2 = start_addr, a3 = size, a4=id |
//! |  4  | RemoteHfenceGvma          | a0-a1 = hart_mask, a2 = start_addr, a3 = size        |
//! |  5  | RemoteHfenceVvmaAsid      | a0-a1 = hart_mask, a2 = start_addr, a3 = size, a4=id |
//! |  6  | RemoteHfenceVvma          | a0-a1 = hart_mask, a2 = start_addr, a3 = size        |
//!
//! Reference: RISC-V SBI Specification v3.0, Chapter 8.

use crate::function::SbiFunction;

const FID_REMOTE_FENCE_I: u64 = 0;
const FID_REMOTE_SFENCE_VMA: u64 = 1;
const FID_REMOTE_SFENCE_VMA_ASID: u64 = 2;
const FID_REMOTE_HFENCE_GVMA_VMID: u64 = 3;
const FID_REMOTE_HFENCE_GVMA: u64 = 4;
const FID_REMOTE_HFENCE_VVMA_ASID: u64 = 5;
const FID_REMOTE_HFENCE_VVMA: u64 = 6;

/// Functions defined by the RFENCE Extension (EID 0x52464E43).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RfenceFunction {
    /// Remote FENCE.I on specified harts (FID #0).
    RemoteFenceI { hart_mask: u64, hart_mask_base: u64 },
    /// Remote SFENCE.VMA on specified harts (FID #1).
    RemoteSfenceVma {
        hart_mask: u64,
        hart_mask_base: u64,
        start_addr: u64,
        size: u64,
    },
    /// Remote SFENCE.VMA with ASID on specified harts (FID #2).
    RemoteSfenceVmaAsid {
        hart_mask: u64,
        hart_mask_base: u64,
        start_addr: u64,
        size: u64,
        asid: u64,
    },
    /// Remote HFENCE.GVMA with VMID on specified harts (FID #3).
    RemoteHfenceGvmaVmid {
        hart_mask: u64,
        hart_mask_base: u64,
        start_addr: u64,
        size: u64,
        vmid: u64,
    },
    /// Remote HFENCE.GVMA on specified harts (FID #4).
    RemoteHfenceGvma {
        hart_mask: u64,
        hart_mask_base: u64,
        start_addr: u64,
        size: u64,
    },
    /// Remote HFENCE.VVMA with ASID on specified harts (FID #5).
    RemoteHfenceVvmaAsid {
        hart_mask: u64,
        hart_mask_base: u64,
        start_addr: u64,
        size: u64,
        asid: u64,
    },
    /// Remote HFENCE.VVMA on specified harts (FID #6).
    RemoteHfenceVvma {
        hart_mask: u64,
        hart_mask_base: u64,
        start_addr: u64,
        size: u64,
    },
}

impl RfenceFunction {
    /// Parses an `RfenceFunction` from the saved register state.
    pub fn from_regs(args: &[u64]) -> crate::error::Result<Self> {
        match args[6] {
            FID_REMOTE_FENCE_I => Ok(RfenceFunction::RemoteFenceI {
                hart_mask: args[0],
                hart_mask_base: args[1],
            }),
            FID_REMOTE_SFENCE_VMA => Ok(RfenceFunction::RemoteSfenceVma {
                hart_mask: args[0],
                hart_mask_base: args[1],
                start_addr: args[2],
                size: args[3],
            }),
            FID_REMOTE_SFENCE_VMA_ASID => Ok(RfenceFunction::RemoteSfenceVmaAsid {
                hart_mask: args[0],
                hart_mask_base: args[1],
                start_addr: args[2],
                size: args[3],
                asid: args[4],
            }),
            FID_REMOTE_HFENCE_GVMA_VMID => Ok(RfenceFunction::RemoteHfenceGvmaVmid {
                hart_mask: args[0],
                hart_mask_base: args[1],
                start_addr: args[2],
                size: args[3],
                vmid: args[4],
            }),
            FID_REMOTE_HFENCE_GVMA => Ok(RfenceFunction::RemoteHfenceGvma {
                hart_mask: args[0],
                hart_mask_base: args[1],
                start_addr: args[2],
                size: args[3],
            }),
            FID_REMOTE_HFENCE_VVMA_ASID => Ok(RfenceFunction::RemoteHfenceVvmaAsid {
                hart_mask: args[0],
                hart_mask_base: args[1],
                start_addr: args[2],
                size: args[3],
                asid: args[4],
            }),
            FID_REMOTE_HFENCE_VVMA => Ok(RfenceFunction::RemoteHfenceVvma {
                hart_mask: args[0],
                hart_mask_base: args[1],
                start_addr: args[2],
                size: args[3],
            }),
            _ => Err(crate::error::Error::NotSupported),
        }
    }
}

impl SbiFunction for RfenceFunction {
    fn a6(&self) -> u64 {
        match self {
            RfenceFunction::RemoteFenceI { .. } => FID_REMOTE_FENCE_I,
            RfenceFunction::RemoteSfenceVma { .. } => FID_REMOTE_SFENCE_VMA,
            RfenceFunction::RemoteSfenceVmaAsid { .. } => FID_REMOTE_SFENCE_VMA_ASID,
            RfenceFunction::RemoteHfenceGvmaVmid { .. } => FID_REMOTE_HFENCE_GVMA_VMID,
            RfenceFunction::RemoteHfenceGvma { .. } => FID_REMOTE_HFENCE_GVMA,
            RfenceFunction::RemoteHfenceVvmaAsid { .. } => FID_REMOTE_HFENCE_VVMA_ASID,
            RfenceFunction::RemoteHfenceVvma { .. } => FID_REMOTE_HFENCE_VVMA,
        }
    }

    fn a0(&self) -> u64 {
        match self {
            RfenceFunction::RemoteFenceI { hart_mask, .. }
            | RfenceFunction::RemoteSfenceVma { hart_mask, .. }
            | RfenceFunction::RemoteSfenceVmaAsid { hart_mask, .. }
            | RfenceFunction::RemoteHfenceGvmaVmid { hart_mask, .. }
            | RfenceFunction::RemoteHfenceGvma { hart_mask, .. }
            | RfenceFunction::RemoteHfenceVvmaAsid { hart_mask, .. }
            | RfenceFunction::RemoteHfenceVvma { hart_mask, .. } => *hart_mask,
        }
    }

    fn a1(&self) -> u64 {
        match self {
            RfenceFunction::RemoteFenceI { hart_mask_base, .. }
            | RfenceFunction::RemoteSfenceVma { hart_mask_base, .. }
            | RfenceFunction::RemoteSfenceVmaAsid { hart_mask_base, .. }
            | RfenceFunction::RemoteHfenceGvmaVmid { hart_mask_base, .. }
            | RfenceFunction::RemoteHfenceGvma { hart_mask_base, .. }
            | RfenceFunction::RemoteHfenceVvmaAsid { hart_mask_base, .. }
            | RfenceFunction::RemoteHfenceVvma { hart_mask_base, .. } => *hart_mask_base,
        }
    }

    fn a2(&self) -> u64 {
        match self {
            RfenceFunction::RemoteFenceI { .. } => 0,
            RfenceFunction::RemoteSfenceVma { start_addr, .. }
            | RfenceFunction::RemoteSfenceVmaAsid { start_addr, .. }
            | RfenceFunction::RemoteHfenceGvmaVmid { start_addr, .. }
            | RfenceFunction::RemoteHfenceGvma { start_addr, .. }
            | RfenceFunction::RemoteHfenceVvmaAsid { start_addr, .. }
            | RfenceFunction::RemoteHfenceVvma { start_addr, .. } => *start_addr,
        }
    }

    fn a3(&self) -> u64 {
        match self {
            RfenceFunction::RemoteFenceI { .. } => 0,
            RfenceFunction::RemoteSfenceVma { size, .. }
            | RfenceFunction::RemoteSfenceVmaAsid { size, .. }
            | RfenceFunction::RemoteHfenceGvmaVmid { size, .. }
            | RfenceFunction::RemoteHfenceGvma { size, .. }
            | RfenceFunction::RemoteHfenceVvmaAsid { size, .. }
            | RfenceFunction::RemoteHfenceVvma { size, .. } => *size,
        }
    }

    fn a4(&self) -> u64 {
        match self {
            RfenceFunction::RemoteSfenceVmaAsid { asid, .. }
            | RfenceFunction::RemoteHfenceVvmaAsid { asid, .. } => *asid,
            RfenceFunction::RemoteHfenceGvmaVmid { vmid, .. } => *vmid,
            _ => 0,
        }
    }
}
