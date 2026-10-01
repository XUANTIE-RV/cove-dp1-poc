// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! TSM system console infrastructure.
//!
//! This module defines the console backend abstraction [`ConsoleBackend`],
//! the global console singleton [`SYSTEM_CONSOLE`], and the accompanying
//! `print!` / `println!` macros. Upper-layer components depend only on the trait
//! abstraction and are unaware of the underlying hardware; a concrete
//! backend (e.g. SBI DBCN) is registered early during boot via
//! [`SystemConsole::attach_backend`].

use crate::sync::Mutex;

/// Console backend abstraction.
///
/// Implementors are responsible for delivering byte sequences to the actual
/// output channel (emit path), and may optionally provide input-reading
/// capability (receive path).
pub trait ConsoleBackend: Sync {
    /// Emits all bytes in `data` to the console.
    ///
    /// Implementations must handle chunking and buffering themselves; the
    /// caller guarantees no upper bound on length.
    fn emit_bytes(&self, data: &[u8]);

    /// Receives console input into `_buf` and returns the number of bytes
    /// actually read.
    ///
    /// The default implementation produces no input (output-only backend)
    /// and always returns 0.
    fn recv_bytes(&self, _buf: &mut [u8]) -> usize {
        0
    }
}

/// System console used by the `print!` / `println!` macros.
///
/// Holds an optional static backend reference internally; when no backend
/// is attached, all output is silently dropped so that invoking the print
/// macros during early boot never crashes.
pub struct SystemConsole {
    backend: Option<&'static dyn ConsoleBackend>,
}

impl SystemConsole {
    /// Constructs an empty console instance with no backend bound yet.
    const fn new() -> Self {
        Self { backend: None }
    }

    /// Reads input from the system console into `buf` and returns the
    /// number of bytes actually read.
    ///
    /// The return value may be less than the buffer length when input is
    /// scarce; returns 0 when no backend is attached.
    pub fn read_input(buf: &mut [u8]) -> usize {
        match SYSTEM_CONSOLE.lock().backend {
            Some(b) => b.recv_bytes(buf),
            None => 0,
        }
    }

    /// Attaches a backend implementation to the system console.
    ///
    /// Usually called early during boot by a concrete backend's
    /// `set_as_console`; only after registration does the output of
    /// `print!` / `println!` actually reach the hardware.
    pub fn attach_backend(backend: &'static dyn ConsoleBackend) {
        SYSTEM_CONSOLE.lock().backend = Some(backend);
    }
}

/// The unique global `SystemConsole` singleton, protected by a spinlock to
/// provide safe interior mutability for the shared console state.
pub static SYSTEM_CONSOLE: Mutex<SystemConsole> = Mutex::new(SystemConsole::new());

/// Formatted-output macro based on the global [`SYSTEM_CONSOLE`] (no-prefix
/// variant).
///
/// Unlike `println!`, `print!` does not prepend the "[TSM] " prefix, which
/// makes it suitable for composing a single logical line together with
/// `println!` (e.g. on the trap handling path) without duplicating the
/// prefix.
#[macro_export]
macro_rules! print {
    ($($args:tt)*) => {{
        use core::fmt::Write;
        let mut guard = $crate::print::SYSTEM_CONSOLE.lock();
        let _ = write!(guard, $($args)*);
    }};
}

/// Whole-line output macro based on the global [`SYSTEM_CONSOLE`] (prefixed
/// variant).
///
/// All output is unconditionally prepended with the "[TSM] " prefix so that
/// TSM logs can be clearly distinguished from the output of other
/// components (RDSM / Host / Guest) on a shared console.
#[macro_export]
macro_rules! println {
    () => {{
        use core::fmt::Write;
        let mut guard = $crate::print::SYSTEM_CONSOLE.lock();
        let _ = write!(guard, "[TSM]\n");
    }};
    ($($args:tt)*) => {{
        use core::fmt::Write;
        let mut guard = $crate::print::SYSTEM_CONSOLE.lock();
        let _ = write!(guard, "[TSM] {}\n", format_args!($($args)*));
    }};
}

impl core::fmt::Write for SystemConsole {
    // Silently drop output when no backend is attached.
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        if let Some(b) = self.backend {
            b.emit_bytes(s.as_bytes());
        }
        Ok(())
    }
}

pub use crate::{print, println};
