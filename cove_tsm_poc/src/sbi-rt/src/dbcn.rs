// SPDX-FileCopyrightText: 2026 DAMO Inc.
// SPDX-FileCopyrightText: 2023 Rivos Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Debug Console Extension (DBCN, EID #0x4442434E), per SBI v3.0 Chapter 12.
//!
//! Replaces the legacy console putchar/getchar (EID #0x01 / #0x02) and allows
//! writing or reading multiple bytes in a single SBI call.

use crate::function::SbiFunction;

/// FID for `sbi_debug_console_write`.
const FID_CONSOLE_WRITE: u64 = 0;
/// FID for `sbi_debug_console_read`.
const FID_CONSOLE_READ: u64 = 1;
/// FID for `sbi_debug_console_write_byte`.
const FID_CONSOLE_WRITE_BYTE: u64 = 2;

/// Functions defined by the Debug Console (DBCN) extension.
#[derive(Clone, Copy, Debug)]
pub enum DbcnFunction {
    /// Write bytes to the debug console from input memory (FID #0).
    ///
    /// This is a non-blocking call that may perform a partial write; the number
    /// of bytes actually written is returned in `sbiret.value`.
    Write {
        /// Number of bytes in the input memory.
        num_bytes: u64,
        /// Lower XLEN bits of the input memory physical base address.
        base_addr_lo: u64,
        /// Upper XLEN bits of the input memory physical base address.
        base_addr_hi: u64,
    },
    /// Read bytes from the debug console into output memory (FID #1).
    Read {
        /// Maximum number of bytes that can be written into the output memory.
        num_bytes: u64,
        /// Lower XLEN bits of the output memory physical base address.
        base_addr_lo: u64,
        /// Upper XLEN bits of the output memory physical base address.
        base_addr_hi: u64,
    },
    /// Write a single byte to the debug console (FID #2). This is a blocking call.
    WriteByte {
        /// The byte to write.
        byte: u8,
    },
}

impl DbcnFunction {
    /// Parses a `DbcnFunction` from the a0-a6 registers of an SBI call.
    pub fn from_regs(args: &[u64]) -> crate::error::Result<Self> {
        match args[6] {
            FID_CONSOLE_WRITE => Ok(DbcnFunction::Write {
                num_bytes: args[0],
                base_addr_lo: args[1],
                base_addr_hi: args[2],
            }),
            FID_CONSOLE_READ => Ok(DbcnFunction::Read {
                num_bytes: args[0],
                base_addr_lo: args[1],
                base_addr_hi: args[2],
            }),
            FID_CONSOLE_WRITE_BYTE => Ok(DbcnFunction::WriteByte {
                byte: args[0] as u8,
            }),
            _ => Err(crate::error::Error::NotSupported),
        }
    }
}

impl SbiFunction for DbcnFunction {
    fn a6(&self) -> u64 {
        match self {
            DbcnFunction::Write { .. } => FID_CONSOLE_WRITE,
            DbcnFunction::Read { .. } => FID_CONSOLE_READ,
            DbcnFunction::WriteByte { .. } => FID_CONSOLE_WRITE_BYTE,
        }
    }

    fn a2(&self) -> u64 {
        match self {
            DbcnFunction::Write { base_addr_hi, .. } => *base_addr_hi,
            DbcnFunction::Read { base_addr_hi, .. } => *base_addr_hi,
            DbcnFunction::WriteByte { .. } => 0,
        }
    }

    fn a1(&self) -> u64 {
        match self {
            DbcnFunction::Write { base_addr_lo, .. } => *base_addr_lo,
            DbcnFunction::Read { base_addr_lo, .. } => *base_addr_lo,
            DbcnFunction::WriteByte { .. } => 0,
        }
    }

    fn a0(&self) -> u64 {
        match self {
            DbcnFunction::Write { num_bytes, .. } => *num_bytes,
            DbcnFunction::Read { num_bytes, .. } => *num_bytes,
            DbcnFunction::WriteByte { byte } => *byte as u64,
        }
    }
}
