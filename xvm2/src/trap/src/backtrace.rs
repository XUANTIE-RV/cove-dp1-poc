// SPDX-FileCopyrightText: 2026 DAMO Inc.
// SPDX-FileCopyrightText: 2023 Rivos Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Frame-pointer unwinding for post-mortem diagnostics.
//!
//! [`FrameWalk`] iterates over the return addresses recorded in the
//! RISC-V frame-pointer chain (s0/fp) of the current hart. Unwinding
//! only needs the word at `fp-8` (the return address) and the word at
//! `fp-16` (the caller's frame pointer), so no debug information is
//! required; rustc must however be built with `force-frame-pointers`
//! so that every activation record keeps s0 meaningful.
//!
//! A walk stays strictly inside the hypervisor stack region described
//! by [`HYP_STACK_BOTTOM`] / [`HYP_STACK_TOP`]; anything else is
//! reported as a diagnostic value instead of being dereferenced.

use super::hyp_layout::{HYP_STACK_BOTTOM, HYP_STACK_TOP};
use alloc::fmt::{Display, Formatter, Result};
use core::arch::asm;
use core::mem::{align_of, size_of};

/// Outcome of unwinding one frame of the chain.
#[derive(Copy, Clone)]
pub enum WalkStep {
    /// A frame unwound successfully; carries its recorded return address.
    FrameAddress(u64),
    /// Walking ran into a frame pointer outside the hypervisor stack.
    DanglingFramePointer(u64),
    /// The chain ended here: a null return address or a frame record
    /// that could not be read.
    Terminated(u64),
}

impl Display for WalkStep {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            // A resolved frame: emit the code address, one per line.
            WalkStep::FrameAddress(addr) => writeln!(f, "frame @ 0x{addr:016x}"),
            // The walk stepped outside the hypervisor stack.
            WalkStep::DanglingFramePointer(addr) => {
                writeln!(f, "unwind aborted: fp 0x{addr:016x} outside stack bounds")
            }
            // The chain bottomed out at a null address or an
            // unreadable frame record.
            WalkStep::Terminated(addr) => {
                writeln!(f, "unwind stopped at 0x{addr:016x}")
            }
        }
    }
}

/// The two words a frame-pointer record keeps immediately below `fp`:
/// the address the callee returns to and the frame pointer of its
/// caller. Both are read in one step so a torn view cannot be produced.
struct FrameRecord {
    /// Return address saved by the callee's prologue.
    return_address: u64,
    /// Frame pointer of the calling function.
    caller_fp: u64,
}

/// Checks that `cursor` is word-aligned and lies inside the inclusive
/// range `[low, high]`.
#[inline]
fn in_stack_region(cursor: u64, low: u64, high: u64) -> bool {
    (low..=high).contains(&cursor) && cursor & (align_of::<u64>() as u64 - 1) == 0
}

/// Reads the live frame pointer (s0/fp) of the current context.
#[inline]
fn read_fp() -> u64 {
    let frame_cursor: u64;
    // SAFETY: moving the fp register into a general-purpose register
    // has no side effects and touches no memory.
    unsafe {
        asm!(
            "mv {cursor}, fp",
            cursor = out(reg) frame_cursor,
            options(nomem, nostack, preserves_flags)
        );
    }
    frame_cursor
}

/// Iterator over the return addresses recorded in the current thread's
/// frame-pointer chain.
pub struct FrameWalk {
    /// Frame cursor for the next step; `None` once the walk has ended.
    cursor: Option<u64>,
    /// Inclusive lower bound of the hypervisor stack region.
    stack_low: u64,
    /// Inclusive upper bound of the hypervisor stack region.
    stack_high: u64,
    /// The stack region viewed as fixed-width word slots.
    slots: &'static [u64],
}

impl FrameWalk {
    /// Creates a trace starting at `frame_cursor`.
    ///
    /// Returns `None` when the cursor is already outside the hypervisor
    /// stack region (for example when called before the hypervisor
    /// stack has been installed).
    fn from_frame_pointer(frame_cursor: u64) -> Option<Self> {
        let (stack_low, stack_high) = (HYP_STACK_BOTTOM, HYP_STACK_TOP);
        if !in_stack_region(frame_cursor, stack_low, stack_high) {
            return None;
        }

        // SAFETY: the hypervisor stack is a fixed, always-mapped region
        // of memory; the slice is only ever read through, and every
        // derived index is bounded by the region size.
        let slots = unsafe {
            core::slice::from_raw_parts(
                stack_low as *const u64,
                ((stack_high - stack_low) / size_of::<u64>() as u64) as usize,
            )
        };

        Some(Self {
            cursor: Some(frame_cursor),
            stack_low,
            stack_high,
            slots,
        })
    }

    /// Loads the frame record stored at `frame_cursor`.
    ///
    /// Returns `None` when the cursor is misaligned, sits too close to
    /// the stack bottom for a full record, or points past the region.
    fn load_frame_record(&self, frame_cursor: u64) -> Option<FrameRecord> {
        // Frame pointers are always word-aligned; the malformed case is
        // rejected rather than probed.
        if frame_cursor & (align_of::<u64>() as u64 - 1) != 0 {
            return None;
        }
        // The record lives at fp-8/fp-16; both words must stay inside
        // the stack region.
        if frame_cursor <= self.stack_low + size_of::<u64>() as u64 {
            return None;
        }

        // Slots are indexed from the bottom of the stack: the return
        // address sits one slot below fp, the saved fp two slots below.
        let fp_slot = ((frame_cursor - self.stack_low) / size_of::<u64>() as u64) as usize;
        Some(FrameRecord {
            return_address: *self.slots.get(fp_slot - 1)?,
            caller_fp: *self.slots.get(fp_slot - 2)?,
        })
    }
}

impl Iterator for FrameWalk {
    type Item = WalkStep;

    fn next(&mut self) -> Option<Self::Item> {
        let frame_cursor = self.cursor?;

        // A cursor that escaped the stack region cannot be trusted.
        if !in_stack_region(frame_cursor, self.stack_low, self.stack_high) {
            self.cursor = None;
            return Some(WalkStep::DanglingFramePointer(frame_cursor));
        }

        match self.load_frame_record(frame_cursor) {
            // A zero return address marks the bottom of the chain.
            Some(record) if record.return_address == 0 => {
                self.cursor = None;
                Some(WalkStep::Terminated(record.caller_fp))
            }
            // Healthy frame: move on to the caller.
            Some(record) => {
                self.cursor = Some(record.caller_fp);
                Some(WalkStep::FrameAddress(record.return_address))
            }
            // Unreadable record: the walk cannot continue.
            None => {
                self.cursor = None;
                Some(WalkStep::Terminated(frame_cursor))
            }
        }
    }
}

/// Captures the current call chain as a [`FrameWalk`] iterator.
pub fn capture_backtrace() -> Option<FrameWalk> {
    FrameWalk::from_frame_pointer(read_fp())
}
