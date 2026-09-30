// SPDX-FileCopyrightText: 2026 DAMO Inc.
// SPDX-FileCopyrightText: 2023 Rivos Inc.
//
// SPDX-License-Identifier: Apache-2.0

use core::fmt;
use riscv_regs::{GeneralPurposeRegisters, GprIndex};

/// Stores the trap context as pushed onto the stack by the trap handler.
#[repr(C)]
pub(crate) struct TrapFrame {
    pub gprs: GeneralPurposeRegisters,
    pub sstatus: u64,
    pub sepc: u64,
}

impl fmt::Display for TrapFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use GprIndex::*;
        writeln!(
            f,
            "SEPC: 0x{:016x}, SSTATUS: 0x{:016x}",
            self.sepc, self.sstatus,
        )?;
        writeln!(
            f,
            "RA:  0x{:016x}, GP:  0x{:016x}, TP:  0x{:016x}, S0:  0x{:016x}",
            self.gprs.reg(RA),
            self.gprs.reg(GP),
            self.gprs.reg(TP),
            self.gprs.reg(S0)
        )?;
        writeln!(
            f,
            "S1:  0x{:016x}, A0:  0x{:016x}, A1:  0x{:016x}, A2:  0x{:016x}",
            self.gprs.reg(S1),
            self.gprs.reg(A0),
            self.gprs.reg(A1),
            self.gprs.reg(A2)
        )?;
        writeln!(
            f,
            "A3:  0x{:016x}, A4:  0x{:016x}, A5:  0x{:016x}, A6:  0x{:016x}",
            self.gprs.reg(A3),
            self.gprs.reg(A4),
            self.gprs.reg(A5),
            self.gprs.reg(A6)
        )?;
        writeln!(
            f,
            "A7:  0x{:016x}, S2:  0x{:016x}, S3:  0x{:016x}, S4:  0x{:016x}",
            self.gprs.reg(A7),
            self.gprs.reg(S2),
            self.gprs.reg(S3),
            self.gprs.reg(S4)
        )?;
        writeln!(
            f,
            "S5:  0x{:016x}, S6:  0x{:016x}, S7:  0x{:016x}, S8:  0x{:016x}",
            self.gprs.reg(S5),
            self.gprs.reg(S6),
            self.gprs.reg(S7),
            self.gprs.reg(S8)
        )?;
        writeln!(
            f,
            "S9:  0x{:016x}, S10: 0x{:016x}, S11: 0x{:016x}, T0:  0x{:016x}",
            self.gprs.reg(S9),
            self.gprs.reg(S10),
            self.gprs.reg(S11),
            self.gprs.reg(T0)
        )?;
        writeln!(
            f,
            "T1:  0x{:016x}, T2:  0x{:016x}, T3:  0x{:016x}, T4:  0x{:016x}",
            self.gprs.reg(T1),
            self.gprs.reg(T2),
            self.gprs.reg(T3),
            self.gprs.reg(T4)
        )?;
        writeln!(
            f,
            "T5:  0x{:016x}, T6:  0x{:016x}, SP:  0x{:016x}",
            self.gprs.reg(T5),
            self.gprs.reg(T6),
            self.gprs.reg(SP)
        )
    }
}
