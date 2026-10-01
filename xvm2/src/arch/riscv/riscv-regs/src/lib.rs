// SPDX-FileCopyrightText: 2026 DAMO Inc.
// SPDX-FileCopyrightText: 2023 Rivos Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! # riscv-regs
//!
//! A zero-cost abstraction layer over the RISC-V privileged architecture
//! register set, targeting RV64 hypervisor firmware. Every accessor compiles
//! down to the underlying CSR/GPR instruction with no runtime overhead.
//!
//! ## Module layout
//!
//! - `regs` - general purpose (x0-x31), floating point, and vector register
//!   file layouts.
//! - `context` - aggregated vCPU register context types (e.g.
//!   `VmCpuRegisters`) used across world switches.
//! - `csrs` - typed access to (H)S-mode control and status registers,
//!   including trap cause decoding into structured enums.
//! - `fence` - memory ordering primitives (DMA/MMIO barriers and the pause
//!   hint).
//! - `csr_addrs` - CSR address constants and IMSIC iselect values.

#![no_std]

// Register file definitions come first: everything else builds on them.
mod regs;

// Composite context and CSR/fence/CSR-address helpers.
mod context;
mod csrs;
mod fence;
mod csr_addrs;

pub use context::*;
pub use csrs::*;
pub use fence::*;
pub use csr_addrs::*;
pub use regs::*;
