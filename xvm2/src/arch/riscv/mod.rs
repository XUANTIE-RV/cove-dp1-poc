// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! RISC-V architecture-specific code.
//!
//! NOTE: This directory has a mixed structure:
//! - `riscv-regs/` is an **independent workspace crate** (has its own Cargo.toml)
//! - All other files (`asm.rs`, `vm_cpu.rs`, `*.S`) belong to the **tsm root crate**
//!
//! The assembly files (`start.S`, `guest.S`, `mem_extable.S`) are included via
//! `global_asm!` in `asm.rs`. `trap.S` is consumed by the `trap` crate via
//! `include_str!`.

#[cfg(all(target_arch = "riscv64", target_os = "none"))]
mod asm;
#[cfg(all(target_arch = "riscv64", target_os = "none"))]
pub(crate) mod vm_cpu;
