// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! SBI Legacy Extensions (EID 0x00–0x08).
//!
//! Legacy extensions predate the modern SBI calling convention and have
//! a different register contract:
//!
//! - **a7** = Extension ID (EID). There is **no** Function ID; a6 is ignored.
//! - **a0** = argument on entry, return value on exit (extension-specific).
//! - **a1** is **not** used for return values (unlike modern SBI where a0=error, a1=value).
//! - All registers except a0 must be preserved across the call.
//!
//! These extensions are **deprecated** in SBI v1.0+ and replaced by modern
//! equivalents (TIME, DBCN, IPI, RFENCE, SRST), but OpenSBI still supports
//! them for backward compatibility.
//!
//! Reference: RISC-V SBI Specification v3.0, Chapter 5.

pub use crate::consts::{
    EXT_LEGACY_CLEAR_IPI, EXT_LEGACY_GETCHAR, EXT_LEGACY_PUTCHAR, EXT_LEGACY_REMOTE_FENCE_I,
    EXT_LEGACY_REMOTE_SFENCE_VMA, EXT_LEGACY_REMOTE_SFENCE_VMA_ASID, EXT_LEGACY_SEND_IPI,
    EXT_LEGACY_SET_TIMER, EXT_LEGACY_SHUTDOWN,
};

/// All Legacy Extension IDs in order (for iteration / probing).
pub const LEGACY_EXTENSION_IDS: [u64; 9] = [
    EXT_LEGACY_SET_TIMER,              // 0x00
    EXT_LEGACY_PUTCHAR,                // 0x01
    EXT_LEGACY_GETCHAR,                // 0x02
    EXT_LEGACY_CLEAR_IPI,              // 0x03
    EXT_LEGACY_SEND_IPI,               // 0x04
    EXT_LEGACY_REMOTE_FENCE_I,         // 0x05
    EXT_LEGACY_REMOTE_SFENCE_VMA,      // 0x06
    EXT_LEGACY_REMOTE_SFENCE_VMA_ASID, // 0x07
    EXT_LEGACY_SHUTDOWN,               // 0x08
];

/// Returns true if the given EID is a Legacy Extension (0x00–0x08).
pub fn is_legacy_extension(eid: u64) -> bool {
    eid <= EXT_LEGACY_SHUTDOWN
}

/// Legacy extension function dispatch.
///
/// Unlike modern SBI extensions, Legacy extensions are identified solely
/// by their EID and have no FID sub-functions. Each EID maps 1:1 to a
/// single operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyFunction {
    /// EID 0x00: Schedule a timer interrupt at the given stime value.
    SetTimer(u64),
    /// EID 0x01: Write a character to the debug console.
    PutChar(u8),
    /// EID 0x02: Read a character from the debug console (returns -1 if none).
    GetChar,
    /// EID 0x03: Clear pending IPIs for the calling hart.
    ClearIpi,
    /// EID 0x04: Send IPI to the harts specified by the hart mask.
    SendIpi(u64),
    /// EID 0x05: Remote FENCE.I on specified harts.
    RemoteFenceI(u64),
    /// EID 0x06: Remote SFENCE.VMA on specified harts.
    RemoteSfenceVma {
        /// Pointer to hart mask
        hart_mask: u64,
        /// Start address
        start: u64,
        /// Size
        size: u64,
    },
    /// EID 0x07: Remote SFENCE.VMA with ASID on specified harts.
    RemoteSfenceVmaAsid {
        /// Pointer to hart mask
        hart_mask: u64,
        /// Start address
        start: u64,
        /// Size
        size: u64,
        /// ASID
        asid: u64,
    },
    /// EID 0x08: System shutdown.
    Shutdown,
}

impl LegacyFunction {
    /// Parse a Legacy function from the saved register state.
    ///
    /// `args[7]` (a7) must be a Legacy EID (0x00–0x08).
    pub fn from_regs(args: &[u64]) -> Option<Self> {
        match args[7] {
            EXT_LEGACY_SET_TIMER => Some(LegacyFunction::SetTimer(args[0])),
            EXT_LEGACY_PUTCHAR => Some(LegacyFunction::PutChar(args[0] as u8)),
            EXT_LEGACY_GETCHAR => Some(LegacyFunction::GetChar),
            EXT_LEGACY_CLEAR_IPI => Some(LegacyFunction::ClearIpi),
            EXT_LEGACY_SEND_IPI => Some(LegacyFunction::SendIpi(args[0])),
            EXT_LEGACY_REMOTE_FENCE_I => Some(LegacyFunction::RemoteFenceI(args[0])),
            EXT_LEGACY_REMOTE_SFENCE_VMA => Some(LegacyFunction::RemoteSfenceVma {
                hart_mask: args[0],
                start: args[1],
                size: args[2],
            }),
            EXT_LEGACY_REMOTE_SFENCE_VMA_ASID => Some(LegacyFunction::RemoteSfenceVmaAsid {
                hart_mask: args[0],
                start: args[1],
                size: args[2],
                asid: args[3],
            }),
            EXT_LEGACY_SHUTDOWN => Some(LegacyFunction::Shutdown),
            _ => None,
        }
    }
}
