// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! SBI ECALL handling module.

#![no_std]

pub mod cove_guest;
#[cfg(all(target_arch = "riscv64", target_os = "none"))]
mod cove_host;
#[cfg(all(target_arch = "riscv64", target_os = "none"))]
mod cove_interrupt;

#[cfg(all(target_arch = "riscv64", target_os = "none"))]
pub use cove_guest::{apply_guest_ecall_resume, handle_guest_exit_ecall};
#[cfg(all(target_arch = "riscv64", target_os = "none"))]
pub use cove_host::handle_cove_host_msg;
#[cfg(all(target_arch = "riscv64", target_os = "none"))]
pub use cove_interrupt::handle_cove_interrupt_msg;
