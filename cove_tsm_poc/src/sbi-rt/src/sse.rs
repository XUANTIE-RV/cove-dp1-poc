// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! SSE Extension (EID 0x535345, ASCII "SSE").
//!
//! The Supervisor Software Events Extension allows the SBI implementation
//! to inject software events into supervisor-mode software. Events
//! preempt all other traps and interrupts, and are delivered only on
//! harts where the supervisor has registered a handler and enabled the
//! event.
//!
//! | FID | Name         | Parameters                                    |
//! |-----|--------------|-----------------------------------------------|
//! |  0  | ReadAttrs    | event_id, base_attr_id, attr_count            |
//! |  1  | WriteAttrs   | event_id, base_attr_id, attr_count            |
//! |  2  | Register     | event_id, handler_entry, handler_stack        |
//! |  3  | Unregister   | event_id                                      |
//! |  4  | Enable       | event_id                                      |
//! |  5  | Disable      | event_id                                      |
//! |  6  | Complete     | event_id, status                              |
//! |  7  | Inject       | event_id, hart_id                             |
//! |  8  | HartUnmask   | (none)                                        |
//! |  9  | HartMask     | (none)                                        |
//!
//! Reference: RISC-V SBI Specification v3.0, Chapter 17.

use crate::function::SbiFunction;

const FID_READ_ATTRS: u64 = 0;
const FID_WRITE_ATTRS: u64 = 1;
const FID_REGISTER: u64 = 2;
const FID_UNREGISTER: u64 = 3;
const FID_ENABLE: u64 = 4;
const FID_DISABLE: u64 = 5;
const FID_COMPLETE: u64 = 6;
const FID_INJECT: u64 = 7;
const FID_HART_UNMASK: u64 = 8;
const FID_HART_MASK: u64 = 9;

/// Functions defined by the SSE Extension (EID 0x535345).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SseFunction {
    /// Read event attributes into shared memory (FID #0).
    ReadAttrs {
        /// Event identifier.
        event_id: u64,
        /// Base attribute identifier.
        base_attr_id: u64,
        /// Number of attributes to read.
        attr_count: u64,
    },
    /// Write event attributes from shared memory (FID #1).
    WriteAttrs {
        /// Event identifier.
        event_id: u64,
        /// Base attribute identifier.
        base_attr_id: u64,
        /// Number of attributes to write.
        attr_count: u64,
    },
    /// Register an event handler (FID #2).
    Register {
        /// Event identifier.
        event_id: u64,
        /// Handler entry point address.
        handler_entry: u64,
        /// Handler stack pointer.
        handler_stack: u64,
    },
    /// Unregister an event handler (FID #3).
    Unregister {
        /// Event identifier.
        event_id: u64,
    },
    /// Enable an event (FID #4).
    Enable {
        /// Event identifier.
        event_id: u64,
    },
    /// Disable an event (FID #5).
    Disable {
        /// Event identifier.
        event_id: u64,
    },
    /// Complete event processing (FID #6).
    Complete {
        /// Event identifier.
        event_id: u64,
        /// Completion status.
        status: u64,
    },
    /// Inject an event to a specific hart (FID #7).
    Inject {
        /// Event identifier.
        event_id: u64,
        /// Target hart identifier.
        hart_id: u64,
    },
    /// Unmask software events on this hart (FID #8).
    HartUnmask,
    /// Mask software events on this hart (FID #9).
    HartMask,
}

impl SseFunction {
    /// Parses an `SseFunction` from the saved register state.
    pub fn from_regs(args: &[u64]) -> crate::error::Result<Self> {
        match args[6] {
            FID_READ_ATTRS => Ok(SseFunction::ReadAttrs {
                event_id: args[0],
                base_attr_id: args[1],
                attr_count: args[2],
            }),
            FID_WRITE_ATTRS => Ok(SseFunction::WriteAttrs {
                event_id: args[0],
                base_attr_id: args[1],
                attr_count: args[2],
            }),
            FID_REGISTER => Ok(SseFunction::Register {
                event_id: args[0],
                handler_entry: args[1],
                handler_stack: args[2],
            }),
            FID_UNREGISTER => Ok(SseFunction::Unregister { event_id: args[0] }),
            FID_ENABLE => Ok(SseFunction::Enable { event_id: args[0] }),
            FID_DISABLE => Ok(SseFunction::Disable { event_id: args[0] }),
            FID_COMPLETE => Ok(SseFunction::Complete {
                event_id: args[0],
                status: args[1],
            }),
            FID_INJECT => Ok(SseFunction::Inject {
                event_id: args[0],
                hart_id: args[1],
            }),
            FID_HART_UNMASK => Ok(SseFunction::HartUnmask),
            FID_HART_MASK => Ok(SseFunction::HartMask),
            _ => Err(crate::error::Error::NotSupported),
        }
    }
}

impl SbiFunction for SseFunction {
    fn a6(&self) -> u64 {
        match self {
            SseFunction::ReadAttrs { .. } => FID_READ_ATTRS,
            SseFunction::WriteAttrs { .. } => FID_WRITE_ATTRS,
            SseFunction::Register { .. } => FID_REGISTER,
            SseFunction::Unregister { .. } => FID_UNREGISTER,
            SseFunction::Enable { .. } => FID_ENABLE,
            SseFunction::Disable { .. } => FID_DISABLE,
            SseFunction::Complete { .. } => FID_COMPLETE,
            SseFunction::Inject { .. } => FID_INJECT,
            SseFunction::HartUnmask => FID_HART_UNMASK,
            SseFunction::HartMask => FID_HART_MASK,
        }
    }

    fn a0(&self) -> u64 {
        match self {
            SseFunction::ReadAttrs { event_id, .. } => *event_id,
            SseFunction::WriteAttrs { event_id, .. } => *event_id,
            SseFunction::Register { event_id, .. } => *event_id,
            SseFunction::Unregister { event_id } => *event_id,
            SseFunction::Enable { event_id } => *event_id,
            SseFunction::Disable { event_id } => *event_id,
            SseFunction::Complete { event_id, .. } => *event_id,
            SseFunction::Inject { event_id, .. } => *event_id,
            SseFunction::HartUnmask | SseFunction::HartMask => 0,
        }
    }

    fn a1(&self) -> u64 {
        match self {
            SseFunction::ReadAttrs { base_attr_id, .. } => *base_attr_id,
            SseFunction::WriteAttrs { base_attr_id, .. } => *base_attr_id,
            SseFunction::Register { handler_entry, .. } => *handler_entry,
            SseFunction::Complete { status, .. } => *status,
            SseFunction::Inject { hart_id, .. } => *hart_id,
            _ => 0,
        }
    }

    fn a2(&self) -> u64 {
        match self {
            SseFunction::ReadAttrs { attr_count, .. } => *attr_count,
            SseFunction::WriteAttrs { attr_count, .. } => *attr_count,
            SseFunction::Register { handler_stack, .. } => *handler_stack,
            _ => 0,
        }
    }
}
