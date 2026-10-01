// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! Assembly inclusion glue module.
//!
//! Embeds the stand-alone assembly sources under `asm/` into the TSM
//! binary via [`global_asm!`], injecting the link-time constants each
//! source expects:
//!
//! * `asm/start.S` - boot hart entry point; receives the
//!   VA-mapped hypervisor stack top (`HYP_STACK_TOP`).
//! * `asm/mem_extable.S` - fault-tolerant guest/user memory copy
//!   routines; takes no parameters.
//!
//! `asm/guest.S` and `asm/trap.S` are deliberately *not* embedded here:
//! they consume structure-offset parameters and are pulled in by
//! `vm_cpu.rs` and the `trap` crate respectively.

use core::arch::global_asm;

use trap::hyp_layout::HYP_STACK_TOP;

global_asm!(include_str!("asm/start.S"), HYP_STACK_TOP = const HYP_STACK_TOP);
global_asm!(include_str!("asm/mem_extable.S"));
