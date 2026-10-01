// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! HS-mode interrupt handling.

#![no_std]

#[cfg(all(target_arch = "riscv64", target_os = "none"))]
use riscv_regs::Interrupt;
#[cfg(all(target_arch = "riscv64", target_os = "none"))]
use riscv_regs::{stopei, RiscvCsrInterface, CSR};

/// IMSIC external interrupt IDs handled at HS-level.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImsicInterruptId {
    Ipi = 1,
}

impl ImsicInterruptId {
    pub fn from_raw(id: u64) -> Option<Self> {
        match id {
            1 => Some(ImsicInterruptId::Ipi),
            _ => None,
        }
    }
}

/// Claims and returns the next pending interrupt from the IMSIC STOPEI register.
#[cfg(all(target_arch = "riscv64", target_os = "none"))]
#[allow(dead_code)]
fn next_pending_interrupt() -> Option<ImsicInterruptId> {
    let raw_id = CSR.stopei.atomic_replace(0) >> stopei::interrupt_id.shift;
    ImsicInterruptId::from_raw(raw_id)
}

/// Claims and discards the next pending interrupt from IMSIC, regardless
/// of whether it is a known interrupt ID.  Returns the raw ID, or None
/// if nothing is pending.
#[cfg(all(target_arch = "riscv64", target_os = "none"))]
fn claim_any_pending_interrupt() -> Option<u64> {
    let raw = CSR.stopei.atomic_replace(0) >> stopei::interrupt_id.shift;
    if raw == 0 {
        None
    } else {
        Some(raw)
    }
}

/// Attempts to handle an interrupt, returning true if the interrupt was successfully handled.
#[cfg(all(target_arch = "riscv64", target_os = "none"))]
pub fn handle_interrupt(irq: Interrupt) -> bool {
    match irq {
        Interrupt::SupervisorExternal => {
            // Drain all pending IMSIC interrupts.  Unknown IDs are
            // silently acknowledged so that TSM does not panic on
            // spurious or Guest-related external interrupts.
            while let Some(_id) = claim_any_pending_interrupt() {
                // IPI (id==1) or any other: just acknowledge.
            }
            true
        }
        Interrupt::SupervisorTimer => {
            // Timer interrupt during TSM execution (e.g. between guest exits).
            // Just acknowledge it — the host will handle rescheduling.
            true
        }
        Interrupt::SupervisorSoft => {
            // Software interrupt (legacy IPI) pending when sie is enabled.
            // In AIA/CoVE, IPIs go via IMSIC (external), but SSIP can be
            // residually set by RDSM TSSA restore or MTTCG timing. Clear
            // the pending bit and continue — host will re-check after TeeRet.
            #[cfg(all(target_arch = "riscv64", target_os = "none"))]
            unsafe {
                core::arch::asm!("csrci sip, 2");
            }
            true
        }
        _ => false,
    }
}
