// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! TSM root library crate — thin integration shell.
//!
//! After the crate extraction refactoring, the core logic lives in dedicated
//! workspace crates:
//!
//! | Crate       | Responsibility                              |
//! |-------------|---------------------------------------------|
//! | `tvm`       | Trusted VM state, page tables, shmem, guest |
//! | `ecall`     | SBI ECALL dispatch (host / guest / interrupt)|
//! | `interrupt` | HS-mode interrupt handling                   |
//! | `trap`      | Trap vector, backtrace, hyp_layout           |
//! | `vcpu`      | vCPU state machine                          |
//! | `sbi-rt`    | SBI protocol types & ecall primitives        |
//! | `utils`     | Print, abort, console, sync utilities        |
//!
//! This lib target exposes the `arch` startup assembly (bare-metal only).

#![no_std]

#[cfg(all(target_arch = "riscv64", target_os = "none"))]
extern crate alloc;

// --- Hardware-dependent modules: bare-metal only ---
#[cfg(all(target_arch = "riscv64", target_os = "none"))]
mod arch;
