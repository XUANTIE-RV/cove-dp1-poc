// SPDX-FileCopyrightText: 2026 DAMO Inc.
// SPDX-FileCopyrightText: 2023 Rivos Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! COVG (CoVE Guest) SBI extension protocol types.
//!
//! The COVG extension provides functions that a TVM guest uses to manage
//! its own memory sharing, MMIO regions, and external interrupt permissions.
//! These calls are issued from VS-mode inside the TVM and handled by the TSM.

use crate::error::*;
use crate::function::*;

/// Functions provided by the COVE Guest extension (EID 0x434F5647 "COVG").
#[derive(Copy, Clone, Debug)]
pub enum CoveGuestFunction {
    /// Declares a guest physical address range as an MMIO region.
    ///
    /// a6 = 0
    AddMmioRegion {
        /// a0 = base address of the MMIO region
        addr: u64,
        /// a1 = length of the MMIO region in bytes
        len: u64,
    },
    /// Removes a previously declared MMIO region.
    ///
    /// a6 = 1
    RemoveMmioRegion {
        /// a0 = base address of the MMIO region
        addr: u64,
        /// a1 = length of the MMIO region in bytes
        len: u64,
    },
    /// Marks a guest physical address range as shared with the host.
    /// The host may then map these pages for communication.
    ///
    /// a6 = 2
    ShareMemory {
        /// a0 = base address of the memory region
        addr: u64,
        /// a1 = length of the memory region in bytes
        len: u64,
    },
    /// Revokes sharing of a guest physical address range.
    ///
    /// a6 = 3
    UnshareMemory {
        /// a0 = base address of the memory region
        addr: u64,
        /// a1 = length of the memory region in bytes
        len: u64,
    },
    /// Allows the host to inject the specified external interrupt into this vCPU.
    /// Pass `interrupt_id = u64::MAX` to allow all external interrupts.
    ///
    /// a6 = 4
    AllowExternalInterrupt {
        /// a0 = interrupt identity number (or u64::MAX for all)
        interrupt_id: u64,
    },
    /// Denies the host from injecting the specified external interrupt.
    /// Pass `interrupt_id = u64::MAX` to deny all external interrupts.
    ///
    /// a6 = 5
    DenyExternalInterrupt {
        /// a0 = interrupt identity number (or u64::MAX for all)
        interrupt_id: u64,
    },
}

impl CoveGuestFunction {
    /// Attempts to parse `Self` from the register values passed in `a0-a7`.
    pub fn from_regs(args: &[u64]) -> Result<Self> {
        use CoveGuestFunction::*;
        match args[6] {
            0 => Ok(AddMmioRegion {
                addr: args[0],
                len: args[1],
            }),
            1 => Ok(RemoveMmioRegion {
                addr: args[0],
                len: args[1],
            }),
            2 => Ok(ShareMemory {
                addr: args[0],
                len: args[1],
            }),
            3 => Ok(UnshareMemory {
                addr: args[0],
                len: args[1],
            }),
            4 => Ok(AllowExternalInterrupt {
                interrupt_id: args[0],
            }),
            5 => Ok(DenyExternalInterrupt {
                interrupt_id: args[0],
            }),
            _ => Err(Error::NotSupported),
        }
    }
}

impl SbiFunction for CoveGuestFunction {
    fn a6(&self) -> u64 {
        use CoveGuestFunction::*;
        match self {
            AddMmioRegion { .. } => 0,
            RemoveMmioRegion { .. } => 1,
            ShareMemory { .. } => 2,
            UnshareMemory { .. } => 3,
            AllowExternalInterrupt { .. } => 4,
            DenyExternalInterrupt { .. } => 5,
        }
    }

    fn a0(&self) -> u64 {
        use CoveGuestFunction::*;
        match self {
            AddMmioRegion { addr, .. } => *addr,
            RemoveMmioRegion { addr, .. } => *addr,
            ShareMemory { addr, .. } => *addr,
            UnshareMemory { addr, .. } => *addr,
            AllowExternalInterrupt { interrupt_id } => *interrupt_id,
            DenyExternalInterrupt { interrupt_id } => *interrupt_id,
        }
    }

    fn a1(&self) -> u64 {
        use CoveGuestFunction::*;
        match self {
            AddMmioRegion { len, .. } => *len,
            RemoveMmioRegion { len, .. } => *len,
            ShareMemory { len, .. } => *len,
            UnshareMemory { len, .. } => *len,
            _ => 0,
        }
    }
}
