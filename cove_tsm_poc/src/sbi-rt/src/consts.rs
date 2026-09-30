// SPDX-FileCopyrightText: 2026 DAMO Inc.
// SPDX-FileCopyrightText: 2023 Rivos Inc.
//
// SPDX-License-Identifier: Apache-2.0

// Legacy Extension constants (SBI v0.1, EID 0x00-0x08)
pub const EXT_LEGACY_SET_TIMER: u64 = 0x00;
pub const EXT_LEGACY_PUTCHAR: u64 = 0x01;
pub const EXT_LEGACY_GETCHAR: u64 = 0x02;
pub const EXT_LEGACY_CLEAR_IPI: u64 = 0x03;
pub const EXT_LEGACY_SEND_IPI: u64 = 0x04;
pub const EXT_LEGACY_REMOTE_FENCE_I: u64 = 0x05;
pub const EXT_LEGACY_REMOTE_SFENCE_VMA: u64 = 0x06;
pub const EXT_LEGACY_REMOTE_SFENCE_VMA_ASID: u64 = 0x07;
pub const EXT_LEGACY_SHUTDOWN: u64 = 0x08;

// Standard Extension constants
pub const EXT_BASE: u64 = 0x10; // Base Extension (SBI v3.0 Ch.4)
#[allow(unused)]
pub const EXT_PUT_CHAR: u64 = EXT_LEGACY_PUTCHAR; // Alias for backward compat
pub const EXT_TIME: u64 = 0x54494D45; // TIME (Timer Extension, SBI v3.0 Ch.6)
pub const EXT_IPI: u64 = 0x735049; // sPI (IPI Extension, SBI v3.0 Ch.7)
pub const EXT_RFENCE: u64 = 0x52464E43; // RFNC (RFENCE Extension, SBI v3.0 Ch.8)
pub const EXT_HSM: u64 = 0x48534D; // HSM (Hart State Management, SBI v3.0 Ch.9)
pub const EXT_SRST: u64 = 0x53525354; // SRST (System Reset, SBI v3.0 Ch.10)
pub const EXT_PMU: u64 = 0x504D55; // PMU (Performance Monitoring Unit, SBI v3.0 Ch.11)
pub const EXT_SUSP: u64 = 0x53555350; // SUSP (System Suspend, SBI v3.0 Ch.13)
pub const EXT_CPPC: u64 = 0x43505043; // CPPC (Collaborative Processor Performance Control, SBI v3.0 Ch.14)
pub const EXT_FWFT: u64 = 0x46574654; // FWFT (Firmware Features, SBI v3.0 Ch.18)
pub const EXT_DBTR: u64 = 0x44425452; // DBTR (Debug Triggers, SBI v3.0 Ch.19)
pub const EXT_MPXY: u64 = 0x4D505859; // MPXY (Message Proxy, SBI v3.0 Ch.20)
pub const EXT_SSE: u64 = 0x535345; // SSE (Supervisor Software Events, SBI v3.0 Ch.17)
pub const EXT_STA: u64 = 0x535441; // STA (Steal-time Accounting, SBI v3.0 Ch.16)
pub const EXT_DBCN: u64 = 0x4442434E; // DBCN
pub const EXT_NACL: u64 = 0x4E41434C; // NACL (Nested Acceleration, SBI v3.0 Ch.15)
pub const EXT_COVE_HOST: u64 = 0x434F5648; // COVH
pub const EXT_COVE_INTERRUPT: u64 = 0x434F5649; // COVI
pub const EXT_COVE_GUEST: u64 = 0x434F5647; // COVG
pub const EXT_TEE_RET: u64 = 0x434F5650;

pub const EXT_VENDOR_RANGE_START: u64 = 0x09000000;
pub const EXT_VENDOR_RANGE_END: u64 = 0x09FFFFFF;

pub const SBI_SUCCESS: i64 = 0;
pub const SBI_ERR_INVALID_ADDRESS: i64 = -5;
