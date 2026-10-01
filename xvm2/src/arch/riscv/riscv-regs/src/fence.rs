// SPDX-FileCopyrightText: 2026 DAMO Inc.
// SPDX-FileCopyrightText: 2023 Rivos Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Memory ordering primitives for RV64 platforms.
//!
//! Wraps the RISC-V `fence` instruction with intent-revealing helpers for
//! DMA and MMIO ordering, plus the `pause` spin-loop hint. Peripherals are
//! assumed to be cache coherent with respect to DMA traffic. On non-RV64
//! (host test) builds every helper degrades to a no-op so callers need no
//! conditional compilation of their own.

// ---------------------------------------------------------------------------
// Host / non-target builds: every barrier is a no-op stub so that unit tests
// and host-side tooling can link against the same API surface.
// ---------------------------------------------------------------------------

/// No-op stand-in for the store-store barrier on non-RV64 builds.
#[cfg(not(any(target_arch = "riscv64", target_os = "none")))]
pub fn dma_wmb() {}

/// No-op stand-in for the load-load barrier on non-RV64 builds.
#[cfg(not(any(target_arch = "riscv64", target_os = "none")))]
pub fn dma_rmb() {}

/// No-op stand-in for the memory-to-IO store barrier on non-RV64 builds.
#[cfg(not(any(target_arch = "riscv64", target_os = "none")))]
pub fn mmio_wmb() {}

/// No-op stand-in for the IO-to-memory load barrier on non-RV64 builds.
#[cfg(not(any(target_arch = "riscv64", target_os = "none")))]
pub fn mmio_rmb() {}

/// No-op stand-in for the spin-loop hint on non-RV64 builds.
#[cfg(not(any(target_arch = "riscv64", target_os = "none")))]
pub fn pause() {}

// ---------------------------------------------------------------------------
// Bare-metal RV64 builds: real fence instructions.
//
// Safety note shared by all asm blocks below: a `fence` instruction performs
// no memory access by itself; its sole effect is to constrain the visible
// ordering of the loads/stores around it.
// ---------------------------------------------------------------------------

#[cfg(all(target_os = "none", target_arch = "riscv64"))]
use core::arch::asm;

/// Guarantees that all earlier memory stores complete before any later
/// memory store (store-store barrier).
#[cfg(all(target_os = "none", target_arch = "riscv64"))]
pub fn dma_wmb() {
    unsafe { asm!("fence w,w") };
}

/// Guarantees that all earlier memory loads complete before any later
/// memory load (load-load barrier).
#[cfg(all(target_os = "none", target_arch = "riscv64"))]
pub fn dma_rmb() {
    unsafe { asm!("fence r,r") };
}

/// Guarantees that all earlier memory stores complete before any later
/// device (IO) store.
#[cfg(all(target_os = "none", target_arch = "riscv64"))]
pub fn mmio_wmb() {
    // NOTE: an MMIO write issued inside a critical section may additionally
    // require `fence o,w`, depending on how the lock release store is
    // ordered against the device store.
    unsafe { asm!("fence w,o") };
}

/// Guarantees that all earlier device (IO) loads complete before any later
/// memory load.
#[cfg(all(target_os = "none", target_arch = "riscv64"))]
pub fn mmio_rmb() {
    unsafe { asm!("fence i,r") };
}

/// Spin-loop hint: asks the core to briefly throttle instruction retirement,
/// reducing power and contention while busy-waiting.
#[cfg(all(target_os = "none", target_arch = "riscv64"))]
pub fn pause() {
    // NOTE: encoded as a raw word because the toolchain does not yet accept
    // the `pause` mnemonic directly.
    unsafe { asm!(".word 0x0100000f") };
}

// ---------------------------------------------------------------------------
// Hypervisor fence instructions (`.insn` encoding because LLVM does not
// accept the mnemonics as first-class instructions at this time).
// ---------------------------------------------------------------------------

/// Flush all G-stage TLB entries on the **local** hart.
///
/// Encoded as `.insn r 0x73, 0, 0x31, x0, x0, x0` (HFENCE.GVMA with
/// rs1 = x0, rs2 = x0 — full flush).
///
/// # Safety
/// Must be called in HS-mode.
#[macro_export]
macro_rules! hfence_gvma {
    () => {
        core::arch::asm!(".insn r 0x73, 0, 0x31, x0, x0, x0")
    };
}

/// Flush all VS-stage TLB entries on the **local** hart.
///
/// Encoded as `.insn r 0x73, 0, 0x11, x0, x0, x0` (HFENCE.VVMA with
/// rs1 = x0, rs2 = x0 — full flush).
///
/// # Safety
/// Must be called in HS-mode.
#[macro_export]
macro_rules! hfence_vvma {
    () => {
        core::arch::asm!(".insn r 0x73, 0, 0x11, x0, x0, x0")
    };
}
