// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! Execution-termination primitive for the DAMO TSM runtime.
//!
//! This module exposes a single entry point, [`abort`], which permanently
//! parks the calling hart (or thread) and never returns. It is the final
//! landing point for unrecoverable firmware failures: the panic handler and
//! the allocation-error handler both funnel into it once diagnostics have
//! been emitted.
//!
//! Two mutually exclusive `cfg` variants are provided:
//!
//! - On the bare-metal RISC-V target (`riscv64` + `target_os = "none"`),
//!   the hart is halted by spinning on the `wfi` instruction, keeping power
//!   consumption minimal while remaining permanently parked.
//! - On any target other than that bare-metal combination (hosted unit
//!   tests, host-side tooling, OS-backed RISC-V such as riscv64-linux, or
//!   bare-metal non-RISC-V such as x86_64-none), the function simply
//!   panics, which lets test harnesses observe the abort via
//!   `#[should_panic]`.

/// Terminates the current thread permanently; non-bare-metal variant.
///
/// On any target other than the bare-metal RISC-V combination
/// (`riscv64` + `target_os = "none"`) there is no hart to park, so the most
/// faithful equivalent of "stop forever" is an immediate panic, which
/// unwinds or aborts according to the target's panic strategy.
#[cfg(not(all(target_arch = "riscv64", target_os = "none")))]
pub fn abort() -> ! {
    panic!();
}

#[cfg(all(target_arch = "riscv64", target_os = "none"))]
use core::arch::asm;

/// Terminates the current hart permanently; bare-metal RISC-V variant.
///
/// The hart is parked in a `wfi` loop and will never resume useful work,
/// regardless of how many wakeup events arrive.
#[cfg(all(target_arch = "riscv64", target_os = "none"))]
pub fn abort() -> ! {
    loop {
        // Safety:
        //
        // - `wfi` (Wait For Interrupt) is an architecturally defined RISC-V
        //   hint instruction. It merely suggests that the hart may stall
        //   until an interrupt (or other implementation-defined wakeup
        //   event) becomes pending; it is always legal for the hardware to
        //   treat it as a no-op.
        // - The instruction reads no memory and writes no memory, and it
        //   does not touch the stack, so `options(nomem, nostack)` is an
        //   accurate description of its effects. No Rust-visible state is
        //   modified, hence no aliasing or validity invariants can be
        //   violated.
        // - Even if the hart wakes up (e.g. a pending interrupt with
        //   interrupts globally disabled), control falls through to the
        //   next iteration of the enclosing `loop`, so this function can
        //   never return and the declared `!` return type is upheld.
        unsafe {
            asm!("wfi", options(nomem, nostack));
        }
    }
}
