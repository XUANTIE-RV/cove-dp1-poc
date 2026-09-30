// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! SBI-backed console drivers.
//!
//! Two [`ConsoleBackend`] implementations live here:
//!
//! * [`DbcnConsole`] — buffered writes through the Debug Console (DBCN)
//!   extension of SBI v2.0+, using a caller-provided staging buffer so
//!   that every firmware call presents a stable, contiguous window.
//! * [`PutCharConsoleV01`] — the legacy per-character console of SBI
//!   v0.1, kept as a fallback for older firmwares.
//!
//! Firmware failures are swallowed by both drivers: once output is being
//! written there is no remaining channel on which an error could be
//! reported, and a misbehaving console must never take the TSM down.

use crate::print::{ConsoleBackend, SystemConsole};
use crate::sync::{Mutex, Once};
use sbi_rt::{ecall_send, DbcnFunction, SbiMessage};

/// Pushes `bytes` through the DBCN Write function until the whole buffer
/// has been accepted.
///
/// DBCN Write is a non-blocking, best-effort interface: the reply counts
/// the bytes actually consumed, which may be fewer than requested (or
/// zero when the console is wedged). The remainder is retried from the
/// first byte the firmware has not taken yet.
fn dbcn_write_all(bytes: &[u8]) -> sbi_rt::Result<()> {
    let mut pending = bytes;
    while !pending.is_empty() {
        let request = SbiMessage::DebugConsole(DbcnFunction::Write {
            num_bytes: pending.len() as u64,
            base_addr_lo: pending.as_ptr() as u64,
            base_addr_hi: 0,
        });

        // SAFETY: `pending` is a live, immutable slice for the duration
        // of the call and the firmware only reads from it.
        let accepted = unsafe { ecall_send::<u64>(&request)? } as usize;

        // No progress at all: the console cannot accept more data.
        if accepted == 0 {
            return Err(sbi_rt::Error::Io);
        }
        // Clamp bogus oversized replies so the slice arithmetic stays in
        // bounds; with a conforming firmware this is a plain advance.
        pending = &pending[accepted.min(pending.len())..];
    }
    Ok(())
}

// ---------- DBCN-backed console (SBI v2.0+) ----------

/// Console driver that stages output in a shared buffer and hands it to
/// the firmware through the DBCN extension.
pub struct DbcnConsole {
    staging: Mutex<&'static mut [u8]>,
}

static DBCN_CONSOLE: Once<DbcnConsole> = Once::new();

impl DbcnConsole {
    /// Installs this driver as the system console, using `staging` as
    /// the intermediate buffer for every write.
    pub fn set_as_console(staging: &'static mut [u8]) {
        DBCN_CONSOLE.call_once(|| DbcnConsole {
            staging: Mutex::new(staging),
        });
        SystemConsole::attach_backend(DBCN_CONSOLE.get().unwrap());
    }
}

impl ConsoleBackend for DbcnConsole {
    /// Feeds the byte sequence to the firmware in staging-sized windows.
    fn emit_bytes(&self, data: &[u8]) {
        let mut staging = self.staging.lock();
        let window_cap = staging.len();
        // A zero-length staging area would make the loop below unable to
        // make progress; treat it as the configuration bug it is.
        assert!(window_cap > 0, "console staging buffer must not be empty");

        let mut consumed = 0;
        while consumed < data.len() {
            let window_end = (consumed + window_cap).min(data.len());
            let window = &data[consumed..window_end];
            // The staged slice is exactly as long as the window; the
            // unused tail of the staging area, if any, stays untouched.
            let (staged, _) = staging.split_at_mut(window.len());
            staged.copy_from_slice(window);
            // Errors are dropped: there is nothing sensible left to do
            // when the firmware refuses console output.
            let _ = dbcn_write_all(staged);
            consumed = window_end;
        }
    }
}

// ---------- Legacy per-character console (SBI v0.1) ----------

/// Console driver for the SBI v0.1 legacy console.
pub struct PutCharConsoleV01;

static PUTCHAR_CONSOLE_V01: PutCharConsoleV01 = PutCharConsoleV01;

impl PutCharConsoleV01 {
    /// Installs this driver as the system console.
    pub fn set_as_console() {
        SystemConsole::attach_backend(&PUTCHAR_CONSOLE_V01);
    }
}

impl ConsoleBackend for PutCharConsoleV01 {
    /// Emits the sequence one character at a time.
    fn emit_bytes(&self, data: &[u8]) {
        for &byte in data {
            let request = SbiMessage::PutChar(byte as u64);
            // SAFETY: PutChar carries no pointers and the ecall touches
            // no memory of ours, so this is trivially sound. Errors are
            // ignored — a broken console must not crash the TSM.
            unsafe {
                let _ = ecall_send::<()>(&request);
            }
        }
    }
}
