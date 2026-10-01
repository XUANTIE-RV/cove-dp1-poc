// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! MPXY Extension (EID 0x4D505859, ASCII "MPXY").
//!
//! The Message Proxy Extension provides supervisor-mode software with a
//! generic interface for sending and receiving messages through various
//! message protocol implementations (e.g. RPMI) via SBI calls.
//!
//! | FID | Name                          | Parameters                                    |
//! |-----|-------------------------------|-----------------------------------------------|
//! |  0  | GetShmemSize                  | (none)                                        |
//! |  1  | SetShmem                      | shmem_phys_lo, shmem_phys_hi, flags           |
//! |  2  | GetChannelIds                 | start_index                                   |
//! |  3  | ReadAttributes                | channel_id, base_attr_id, attr_count          |
//! |  4  | WriteAttributes               | channel_id, base_attr_id, attr_count          |
//! |  5  | SendMessageWithResponse       | channel_id, msg_id, msg_data_len              |
//! |  6  | SendMessageWithoutResponse    | channel_id, msg_id, msg_data_len              |
//! |  7  | GetNotificationEvents         | channel_id                                    |
//!
//! Reference: RISC-V SBI Specification v3.0, Chapter 20.

use crate::function::SbiFunction;

const FID_GET_SHMEM_SIZE: u64 = 0;
const FID_SET_SHMEM: u64 = 1;
const FID_GET_CHANNEL_IDS: u64 = 2;
const FID_READ_ATTRIBUTES: u64 = 3;
const FID_WRITE_ATTRIBUTES: u64 = 4;
const FID_SEND_MESSAGE_WITH_RESPONSE: u64 = 5;
const FID_SEND_MESSAGE_WITHOUT_RESPONSE: u64 = 6;
const FID_GET_NOTIFICATION_EVENTS: u64 = 7;

/// Functions defined by the MPXY Extension (EID 0x4D505859).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MpxyFunction {
    /// Query the required shared memory size (FID #0).
    GetShmemSize,
    /// Set the shared memory region for message data exchange (FID #1).
    SetShmem {
        /// Physical address low bits.
        shmem_phys_lo: u64,
        /// Physical address high bits.
        shmem_phys_hi: u64,
        /// Flags (reserved, must be 0).
        flags: u64,
    },
    /// Get list of available channel IDs (FID #2).
    GetChannelIds {
        /// Starting index in the channel list.
        start_index: u64,
    },
    /// Read attributes for a channel (FID #3).
    ReadAttributes {
        /// Channel identifier.
        channel_id: u64,
        /// Base attribute identifier.
        base_attr_id: u64,
        /// Number of attributes to read.
        attr_count: u64,
    },
    /// Write attributes for a channel (FID #4).
    WriteAttributes {
        /// Channel identifier.
        channel_id: u64,
        /// Base attribute identifier.
        base_attr_id: u64,
        /// Number of attributes to write.
        attr_count: u64,
    },
    /// Send a message and wait for response (FID #5).
    SendMessageWithResponse {
        /// Channel identifier.
        channel_id: u64,
        /// Message identifier.
        msg_id: u64,
        /// Length of message data in shared memory.
        msg_data_len: u64,
    },
    /// Send a message without waiting for response (FID #6).
    SendMessageWithoutResponse {
        /// Channel identifier.
        channel_id: u64,
        /// Message identifier.
        msg_id: u64,
        /// Length of message data in shared memory.
        msg_data_len: u64,
    },
    /// Get notification events for a channel (FID #7).
    GetNotificationEvents {
        /// Channel identifier.
        channel_id: u64,
    },
}

impl MpxyFunction {
    /// Parses a `MpxyFunction` from the saved register state.
    pub fn from_regs(args: &[u64]) -> crate::error::Result<Self> {
        match args[6] {
            FID_GET_SHMEM_SIZE => Ok(MpxyFunction::GetShmemSize),
            FID_SET_SHMEM => Ok(MpxyFunction::SetShmem {
                shmem_phys_lo: args[0],
                shmem_phys_hi: args[1],
                flags: args[2],
            }),
            FID_GET_CHANNEL_IDS => Ok(MpxyFunction::GetChannelIds {
                start_index: args[0],
            }),
            FID_READ_ATTRIBUTES => Ok(MpxyFunction::ReadAttributes {
                channel_id: args[0],
                base_attr_id: args[1],
                attr_count: args[2],
            }),
            FID_WRITE_ATTRIBUTES => Ok(MpxyFunction::WriteAttributes {
                channel_id: args[0],
                base_attr_id: args[1],
                attr_count: args[2],
            }),
            FID_SEND_MESSAGE_WITH_RESPONSE => Ok(MpxyFunction::SendMessageWithResponse {
                channel_id: args[0],
                msg_id: args[1],
                msg_data_len: args[2],
            }),
            FID_SEND_MESSAGE_WITHOUT_RESPONSE => Ok(MpxyFunction::SendMessageWithoutResponse {
                channel_id: args[0],
                msg_id: args[1],
                msg_data_len: args[2],
            }),
            FID_GET_NOTIFICATION_EVENTS => Ok(MpxyFunction::GetNotificationEvents {
                channel_id: args[0],
            }),
            _ => Err(crate::error::Error::NotSupported),
        }
    }
}

impl SbiFunction for MpxyFunction {
    fn a6(&self) -> u64 {
        match self {
            MpxyFunction::GetShmemSize => FID_GET_SHMEM_SIZE,
            MpxyFunction::SetShmem { .. } => FID_SET_SHMEM,
            MpxyFunction::GetChannelIds { .. } => FID_GET_CHANNEL_IDS,
            MpxyFunction::ReadAttributes { .. } => FID_READ_ATTRIBUTES,
            MpxyFunction::WriteAttributes { .. } => FID_WRITE_ATTRIBUTES,
            MpxyFunction::SendMessageWithResponse { .. } => FID_SEND_MESSAGE_WITH_RESPONSE,
            MpxyFunction::SendMessageWithoutResponse { .. } => FID_SEND_MESSAGE_WITHOUT_RESPONSE,
            MpxyFunction::GetNotificationEvents { .. } => FID_GET_NOTIFICATION_EVENTS,
        }
    }

    fn a0(&self) -> u64 {
        match self {
            MpxyFunction::GetShmemSize => 0,
            MpxyFunction::SetShmem { shmem_phys_lo, .. } => *shmem_phys_lo,
            MpxyFunction::GetChannelIds { start_index } => *start_index,
            MpxyFunction::ReadAttributes { channel_id, .. } => *channel_id,
            MpxyFunction::WriteAttributes { channel_id, .. } => *channel_id,
            MpxyFunction::SendMessageWithResponse { channel_id, .. } => *channel_id,
            MpxyFunction::SendMessageWithoutResponse { channel_id, .. } => *channel_id,
            MpxyFunction::GetNotificationEvents { channel_id } => *channel_id,
        }
    }

    fn a1(&self) -> u64 {
        match self {
            MpxyFunction::SetShmem { shmem_phys_hi, .. } => *shmem_phys_hi,
            MpxyFunction::ReadAttributes { base_attr_id, .. } => *base_attr_id,
            MpxyFunction::WriteAttributes { base_attr_id, .. } => *base_attr_id,
            MpxyFunction::SendMessageWithResponse { msg_id, .. } => *msg_id,
            MpxyFunction::SendMessageWithoutResponse { msg_id, .. } => *msg_id,
            _ => 0,
        }
    }

    fn a2(&self) -> u64 {
        match self {
            MpxyFunction::SetShmem { flags, .. } => *flags,
            MpxyFunction::ReadAttributes { attr_count, .. } => *attr_count,
            MpxyFunction::WriteAttributes { attr_count, .. } => *attr_count,
            MpxyFunction::SendMessageWithResponse { msg_data_len, .. } => *msg_data_len,
            MpxyFunction::SendMessageWithoutResponse { msg_data_len, .. } => *msg_data_len,
            _ => 0,
        }
    }
}
