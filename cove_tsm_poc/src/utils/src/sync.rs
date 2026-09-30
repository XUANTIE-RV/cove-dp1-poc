// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! # Synchronization Primitives Facade
//!
//! This module is the single entry point for synchronization primitives in
//! the CoVE TSM firmware runtime. All in-tree crates must reference lock
//! types through `utils::sync` instead of depending on third-party crates
//! directly.
//!
//! ## Why the `spin` crate
//!
//! The TSM runs in a RISC-V bare-metal (`riscv64-unknown-none`) environment
//! with the following constraints:
//!
//! - **No OS scheduler**: there is no thread blocking/wakeup mechanism (no
//!   blocking wait available), so busy-waiting is the only viable means of
//!   cross-hart mutual exclusion;
//! - **`no_std` + no heap allocation**: the global allocator is a stub
//!   implementation; all `spin` lock types are statically sized and can be
//!   used directly in `static` global singletons, satisfying the
//!   zero-heap-allocation requirement;
//! - **Very short critical sections**: critical sections on firmware hot
//!   paths usually involve only a few register-level state updates, so the
//!   spinning overhead is acceptable in low-contention scenarios.
//!
//! ## Backend Swappability
//!
//! The facade pattern decouples the backend implementation from its users:
//! if we later switch to a spinlock with exponential backoff, a ticket
//! lock, or a hardware transactional memory scheme, only the re-export
//! targets in this file need to change; no call sites require any
//! modification.
//!
//! ## Usage Guidelines
//!
//! - Prefer [`Once`] for one-shot initialization instead of hand-written
//!   double-checked locking;
//! - Use [`RwLock`] for read-mostly shared state and [`Mutex`] for all
//!   other mutual-exclusion scenarios;
//! - Never perform potentially long-blocking operations (such as console
//!   IO) while holding a lock.

// One-shot initialization primitive
pub use spin::Once;

// Reader-writer lock family
pub use spin::{RwLock, RwLockReadGuard, RwLockWriteGuard};

// Mutual-exclusion lock family
pub use spin::{Mutex, MutexGuard};
