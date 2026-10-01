// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! `utils` — S-mode bare-metal runtime infrastructure library.
//!
//! This crate targets `no_std` code running in RISC-V S-mode. It gathers
//! the low-level capabilities that every component needs but that belong to
//! no specific business module, so upper-layer crates do not have to
//! re-implement them. The overall design separates abstraction from
//! implementation: traits and macros are exposed publicly, while hardware
//! details stay encapsulated inside concrete driver implementations.
//!
//! # Module responsibilities
//!
//! - [`print`]: formatted-output abstraction layer. Defines the
//!   `ConsoleBackend` trait and the global console singleton, and exports the
//!   `print!` / `println!` macros. Upper layers depend only on the
//!   abstraction and are unaware of the underlying device.
//! - [`sbi_console`]: SBI backend implementation of `ConsoleBackend`.
//!   Provides a buffered write channel based on the DBCN extension and a
//!   per-character fallback channel based on SBI v0.1 `PutChar`; both are
//!   installed as the global console via registration functions early during
//!   boot.
//! - [`sync`]: synchronization-primitive facade that re-exports spin-based
//!   lock types such as `Mutex` / `RwLock` / `Once`. No heap allocation, no
//!   scheduler dependency; usable directly in `static` global singletons.
//!   The backend can be swapped wholesale without touching any call site.
//! - [`abort`]: execution-termination path. Provides the platform-agnostic
//!   `abort()`, which enters a `wfi` loop on the bare-metal RISC-V target
//!   and falls back to panic on other targets; reused by the panic and
//!   allocation-error handlers.
//!
//! # Usage notes
//!
//! The console must be explicitly registered by upper layers early during
//! boot (register-before-use); invoking the `print!` family of macros
//! before registration completes produces no output.

#![no_std]
#![feature(alloc_error_handler)]
#![feature(allocator_api)]

/// S-mode execution-termination path: wfi loop or panic fallback.
pub mod abort;
/// Formatted-output abstraction: ConsoleBackend trait + print!/println! macros.
pub mod print;
/// SBI backend console driver (DBCN buffered write / v0.1 PutChar fallback).
pub mod sbi_console;
/// Spin-based synchronization primitives: Mutex / RwLock / Once.
pub mod sync;

// Core types needed by global allocator implementations, re-exported from
// this crate so call sites do not reference core::alloc directly.
pub use core::alloc::{GlobalAlloc, Layout};
