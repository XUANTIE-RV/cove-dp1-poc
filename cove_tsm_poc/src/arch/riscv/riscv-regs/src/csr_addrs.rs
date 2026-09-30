// SPDX-FileCopyrightText: 2026 DAMO Inc.
// SPDX-FileCopyrightText: 2023 Rivos Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! RISC-V CSR address constants and IMSIC iselect values used by the register framework.

// ---------------------------------------------------------------------------
// Trap handling — trap vector, exception PC, cause, trap value, scratch
// ---------------------------------------------------------------------------

/// Supervisor trap vector base address
pub const CSR_S_TVEC: u16 = 0x105;
/// Supervisor exception program counter
pub const CSR_S_EPC: u16 = 0x141;
/// Supervisor trap cause
pub const CSR_S_CAUSE: u16 = 0x142;
/// Supervisor trap value
pub const CSR_S_TVAL: u16 = 0x143;
/// Supervisor scratch register
pub const CSR_S_SCRATCH: u16 = 0x140;

/// Virtual supervisor trap vector base
pub const CSR_VS_TVEC: u16 = 0x205;
/// Virtual supervisor exception program counter
pub const CSR_VS_EPC: u16 = 0x241;
/// Virtual supervisor trap cause
pub const CSR_VS_CAUSE: u16 = 0x242;
/// Virtual supervisor trap value
pub const CSR_VS_TVAL: u16 = 0x243;
/// Virtual supervisor scratch register
pub const CSR_VS_SCRATCH: u16 = 0x240;

/// Hypervisor trap value
pub const CSR_H_TVAL: u16 = 0x643;
/// Hypervisor trap instruction
pub const CSR_H_TINST: u16 = 0x64a;

// ---------------------------------------------------------------------------
// Interrupt control — enable/pending/delegation + AIA indirect window
// ---------------------------------------------------------------------------

/// Supervisor interrupt enable
pub const CSR_S_IE: u16 = 0x104;
/// Supervisor interrupt pending
pub const CSR_S_IP: u16 = 0x144;
/// Supervisor indirect CSR select
pub const CSR_S_ISELECT: u16 = 0x150;
/// Supervisor indirect CSR data
pub const CSR_S_IREG: u16 = 0x151;
/// Supervisor top-claim external interrupt
pub const CSR_S_TOPEI: u16 = 0x15c;
/// Supervisor top interrupt
pub const CSR_S_TOPI: u16 = 0xdb0;

/// Virtual supervisor interrupt enable
pub const CSR_VS_IE: u16 = 0x204;
/// Virtual supervisor interrupt pending
pub const CSR_VS_IP: u16 = 0x244;
/// Virtual supervisor indirect CSR select
pub const CSR_VS_ISELECT: u16 = 0x250;
/// Virtual supervisor indirect CSR data
pub const CSR_VS_IREG: u16 = 0x251;
/// Virtual supervisor top-claim external interrupt
pub const CSR_VS_TOPEI: u16 = 0x25c;
/// Virtual supervisor top interrupt
pub const CSR_VS_TOPI: u16 = 0xeb0;

/// Hypervisor exception delegation
pub const CSR_H_EDELEG: u16 = 0x602;
/// Hypervisor interrupt delegation
pub const CSR_H_IDELEG: u16 = 0x603;
/// Hypervisor interrupt enable
pub const CSR_H_IE: u16 = 0x604;
/// Hypervisor interrupt pending
pub const CSR_H_IP: u16 = 0x644;
/// Hypervisor virtual interrupt pending
pub const CSR_H_VIP: u16 = 0x645;
/// Hypervisor guest external interrupt enable
pub const CSR_H_GEIE: u16 = 0x607;
/// Hypervisor guest external interrupt pending
pub const CSR_H_GEIP: u16 = 0xe12;
/// Hypervisor virtual interrupt control
pub const CSR_H_VICTL: u16 = 0x609;

// ---------------------------------------------------------------------------
// Memory management — address translation
// ---------------------------------------------------------------------------

/// Supervisor address translation and protection
pub const CSR_S_ATP: u16 = 0x180;
/// Virtual supervisor address translation and protection
pub const CSR_VS_ATP: u16 = 0x280;
/// Hypervisor guest address translation and protection
pub const CSR_H_GATP: u16 = 0x680;

// ---------------------------------------------------------------------------
// Status & configuration — privilege status, counter enable, env config
// ---------------------------------------------------------------------------

/// Supervisor status
pub const CSR_S_STATUS: u16 = 0x100;
/// Supervisor counter enable
pub const CSR_S_COUNTEREN: u16 = 0x106;
/// Virtual supervisor status
pub const CSR_VS_STATUS: u16 = 0x200;
/// Hypervisor status
pub const CSR_H_STATUS: u16 = 0x600;
/// Hypervisor environment configuration
pub const CSR_H_ENVCFG: u16 = 0x60a;
/// Hypervisor state enable 0
pub const CSR_H_STATEEN0: u16 = 0x60c;
/// Hypervisor counter enable
pub const CSR_H_COUNTEREN: u16 = 0x606;

// ---------------------------------------------------------------------------
// Timer — timer compare and time delta
// ---------------------------------------------------------------------------

/// Supervisor timer compare
pub const CSR_S_TIMECMP: u16 = 0x14d;
/// Virtual supervisor timer compare
pub const CSR_VS_TIMECMP: u16 = 0x24d;
/// Hypervisor time delta
pub const CSR_H_TIMEDELTA: u16 = 0x605;

// ---------------------------------------------------------------------------
// Vector extension CSRs
// ---------------------------------------------------------------------------

/// Vector start index
pub const CSR_VSTART: u16 = 0x8;
/// Vector control and status
pub const CSR_VCSR: u16 = 0xf;
/// Vector length
pub const CSR_VL: u16 = 0xc20;
/// Vector type configuration
pub const CSR_VTYPE: u16 = 0xc21;
/// Vector register length in bytes
pub const CSR_VLENB: u16 = 0xc22;

// ---------------------------------------------------------------------------
// Performance counters
// ---------------------------------------------------------------------------

/// Cycle counter
pub const CSR_CYCLE: u16 = 0xc00;
/// Wall-clock time
pub const CSR_TIME: u16 = 0xc01;
/// Instructions retired
pub const CSR_INSTRET: u16 = 0xc02;

// ---------------------------------------------------------------------------
// Miscellaneous
// ---------------------------------------------------------------------------

/// Hardware random seed
pub const CSR_SEED: u16 = 0x15;

// ---------------------------------------------------------------------------
// IMSIC indirect register select values
// ---------------------------------------------------------------------------

/// External interrupt delivery enable
pub const ISELECT_EIDELIVERY: u64 = 0x70;
/// External interrupt threshold
pub const ISELECT_EITHRESHOLD: u64 = 0x72;
/// External interrupt pending base
pub const ISELECT_EIP_BASE: u64 = 0x80;
/// External interrupt enable base
pub const ISELECT_EIE_BASE: u64 = 0xc0;
