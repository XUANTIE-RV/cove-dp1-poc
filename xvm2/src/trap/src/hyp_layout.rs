// SPDX-FileCopyrightText: 2026 DAMO Inc.
// SPDX-FileCopyrightText: 2023 Rivos Inc.
//
// SPDX-License-Identifier: Apache-2.0

// VM-Layout of TSM.
//
//
// +-------------------------+ 0x0000_0000_0000_0000
// | 1:1 HwMemoryMap         |
// +-------------------------+ (Highest HwMemoryMap address)
// | (unused)                |
// +-------------------------+ HYP_STACK_BOTTOM (HYP_STACK_TOP - HYP_STACK_SIZE)
// | Hypervisor Stack        |
// +-------------------------+ HYP_STACK_TOP (0xffff_ffff_ffe0_0000)
// | (unused 2Mb)            |
// +-------------------------+ End of Address Space.

// Stack layout constants are defined once in the `config` crate; re-export
// them so existing `trap::hyp_layout::*` users keep working.
pub use config::{HYP_STACK_BOTTOM, HYP_STACK_PAGES, HYP_STACK_SIZE, HYP_STACK_TOP};
