// SPDX-FileCopyrightText: 2026 DAMO Inc.
// SPDX-FileCopyrightText: 2023 Rivos Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Base Extension (EID #0x10), per SBI v3.0 Chapter 4.
//!
//! The base extension is mandatory for all SBI implementations. It provides
//! functions for querying the SBI specification version, implementation
//! details, and probing available extensions.

use crate::function::SbiFunction;

/// FID for `sbi_get_spec_version`.
const FID_GET_SPEC_VERSION: u64 = 0;
/// FID for `sbi_get_impl_id`.
const FID_GET_IMPL_ID: u64 = 1;
/// FID for `sbi_get_impl_version`.
const FID_GET_IMPL_VERSION: u64 = 2;
/// FID for `sbi_probe_extension`.
const FID_PROBE_EXTENSION: u64 = 3;
/// FID for `sbi_get_mvendorid`.
const FID_GET_MVENDORID: u64 = 4;
/// FID for `sbi_get_marchid`.
const FID_GET_MARCHID: u64 = 5;
/// FID for `sbi_get_mimpid`.
const FID_GET_MIMPID: u64 = 6;

/// Functions defined by the Base extension (EID #0x10).
#[derive(Clone, Copy, Debug)]
pub enum BaseFunction {
    /// Get SBI specification version (FID #0).
    ///
    /// Returns the current SBI spec version. Minor number in bits [23:0],
    /// major number in bits [30:24]. Bit 31 is reserved and must be 0.
    GetSpecVersion,
    /// Get SBI implementation ID (FID #1).
    ///
    /// Returns the implementation ID (e.g. 1 = OpenSBI, 4 = RustSBI).
    GetImplId,
    /// Get SBI implementation version (FID #2).
    ///
    /// Returns the implementation-specific version number.
    GetImplVersion,
    /// Probe SBI extension availability (FID #3).
    ///
    /// Returns 0 if the given extension ID is not available, or a non-zero
    /// value if it is available.
    ProbeExtension {
        /// The extension ID to probe.
        extension_id: u64,
    },
    /// Get machine vendor ID (FID #4).
    ///
    /// Returns a value legal for the `mvendorid` CSR.
    GetMvendorId,
    /// Get machine architecture ID (FID #5).
    ///
    /// Returns a value legal for the `marchid` CSR.
    GetMarchId,
    /// Get machine implementation ID (FID #6).
    ///
    /// Returns a value legal for the `mimpid` CSR.
    GetMimpId,
}

impl BaseFunction {
    /// Parses a `BaseFunction` from the a0-a6 registers of an SBI call.
    pub fn from_regs(args: &[u64]) -> crate::error::Result<Self> {
        match args[6] {
            FID_GET_SPEC_VERSION => Ok(BaseFunction::GetSpecVersion),
            FID_GET_IMPL_ID => Ok(BaseFunction::GetImplId),
            FID_GET_IMPL_VERSION => Ok(BaseFunction::GetImplVersion),
            FID_PROBE_EXTENSION => Ok(BaseFunction::ProbeExtension {
                extension_id: args[0],
            }),
            FID_GET_MVENDORID => Ok(BaseFunction::GetMvendorId),
            FID_GET_MARCHID => Ok(BaseFunction::GetMarchId),
            FID_GET_MIMPID => Ok(BaseFunction::GetMimpId),
            _ => Err(crate::error::Error::NotSupported),
        }
    }
}

impl SbiFunction for BaseFunction {
    fn a6(&self) -> u64 {
        match self {
            BaseFunction::GetSpecVersion => FID_GET_SPEC_VERSION,
            BaseFunction::GetImplId => FID_GET_IMPL_ID,
            BaseFunction::GetImplVersion => FID_GET_IMPL_VERSION,
            BaseFunction::ProbeExtension { .. } => FID_PROBE_EXTENSION,
            BaseFunction::GetMvendorId => FID_GET_MVENDORID,
            BaseFunction::GetMarchId => FID_GET_MARCHID,
            BaseFunction::GetMimpId => FID_GET_MIMPID,
        }
    }

    fn a0(&self) -> u64 {
        match self {
            BaseFunction::ProbeExtension { extension_id } => *extension_id,
            _ => 0,
        }
    }
}
