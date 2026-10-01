// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! Lookup support for the exception table.
//!
//! The linker gathers every faultable instruction address emitted by
//! `add_extable` (see `mem_extable.S`) into the allocatable `.extable`
//! section and brackets it with the `__exception_table_begin` /
//! `__exception_table_end` symbols. `handle_trap` consults the lookup
//! below to tell a recoverable guest-access fault apart from a genuine
//! bug.

use core::ptr;

extern "C" {
    static __exception_table_begin: u8;
    static __exception_table_end: u8;
}

/// One exception table slot: the address of an instruction that is
/// allowed to fault in supervisor mode.
///
/// Entries are collected as 8-byte-aligned quads, so this transparent
/// wrapper lets the table be walked exactly like an array of `u64`
/// without building a Rust slice over the section.
#[repr(transparent)]
struct ExceptionEntry {
    fault_pc: u64,
}

/// Returns the half-open pointer range of the exception table.
fn extable_range() -> (*const ExceptionEntry, *const ExceptionEntry) {
    // The linker script guarantees both symbols are 8-byte aligned and
    // bracket exactly the collected entries.
    let begin = ptr::addr_of!(__exception_table_begin) as *const ExceptionEntry;
    let end = ptr::addr_of!(__exception_table_end) as *const ExceptionEntry;
    (begin, end)
}

/// Reports whether `pc` is registered in the exception table.
pub(crate) fn pc_in_extable(pc: u64) -> bool {
    let (mut slot, end) = extable_range();
    while slot < end {
        // SAFETY: `slot` walks the linker-emitted table without ever
        // leaving the `[begin, end)` range; entries are aligned and
        // fully initialized by construction, and only read, never
        // written, through this pointer.
        let entry = unsafe { slot.read() };
        if entry.fault_pc == pc {
            return true;
        }
        slot = unsafe { slot.add(1) };
    }
    false
}
