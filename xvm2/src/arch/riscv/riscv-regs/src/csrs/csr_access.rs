// SPDX-FileCopyrightText: 2026 DAMO Inc.
// SPDX-FileCopyrightText: 2023 Rivos Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Accessor types for RISC-V control and status registers (CSRs).
//!
//! Two accessors are provided here: [`ReadWriteRiscvCsr`] for CSRs that
//! are directly addressable by a 12-bit CSR number, and
//! [`IndirectReadWriteRiscvCsr`] for CSRs that must be reached through
//! an address/data indirection window (e.g. `siselect`/`sireg`).

#[cfg(all(target_os = "none", target_arch = "riscv64"))]
use core::arch::asm;
use core::marker::PhantomData;

use tock_registers::fields::Field;
use tock_registers::interfaces::{Readable, Writeable};
use tock_registers::RegisterLongName;

/// The set of operations available on a RISC-V CSR.
pub trait RiscvCsrInterface {
    type R: RegisterLongName;

    /// Swaps the CSR contents atomically.
    ///
    /// The current CSR value is fetched and replaced by `val_to_set`
    /// in one instruction; the prior value is handed back.
    ///
    /// Maps onto the RISC-V `CSRRW rd, csr, rs1` instruction where
    /// `rs1 = in(reg) val_to_set` and `rd = out(reg) <return value>`.
    fn atomic_replace(&self, val_to_set: u64) -> u64;

    /// Fetches the current CSR contents.
    ///
    /// Maps onto the RISC-V `CSRR rd, csr` instruction where
    /// `rd = out(reg) <return value>`.
    fn get_value(&self) -> u64;

    /// Stores a new value into the CSR.
    ///
    /// Maps onto the RISC-V `CSRW csr, rs` instruction where
    /// `rs = in(reg) val_to_set`.
    fn set_value(&self, val_to_set: u64);

    /// Atomically fetches the CSR and sets every bit present in the
    /// supplied bitmask.
    ///
    /// Maps onto the RISC-V `CSRRS rd, csr, rs1` instruction where
    /// `rs1 = in(reg) bitmask` and `rd = out(reg) <return value>`.
    fn read_and_set_bits(&self, bitmask: u64) -> u64;

    /// Atomically fetches the CSR and clears every bit present in the
    /// supplied bitmask.
    ///
    /// Maps onto the RISC-V `CSRRC rd, csr, rs1` instruction where
    /// `rs1 = in(reg) bitmask` and `rd = out(reg) <return value>`.
    fn read_and_clear_bits(&self, bitmask: u64) -> u64;

    /// Atomically fetches a register field and drives all of its bits
    /// to one.
    ///
    /// Maps onto the RISC-V `CSRRS rd, csr, rs1` instruction, with
    /// `rs1` holding the bitmask derived from the [`Field`].
    ///
    /// Returns the field value observed before the update.
    fn read_and_set_field(&self, field: Field<u64, Self::R>) -> u64 {
        field.read(self.read_and_set_bits(field.mask << field.shift))
    }

    /// Atomically fetches a register field and drives all of its bits
    /// to zero.
    ///
    /// Maps onto the RISC-V `CSRRC rd, csr, rs1` instruction, with
    /// `rs1` holding the bitmask derived from the [`Field`].
    ///
    /// Returns the field value observed before the update.
    fn read_and_clear_field(&self, field: Field<u64, Self::R>) -> u64 {
        field.read(self.read_and_clear_bits(field.mask << field.shift))
    }
}

/// A directly addressable read/write CSR, identified by its 12-bit
/// CSR number `V`.
#[derive(Copy, Clone)]
pub struct ReadWriteRiscvCsr<R: RegisterLongName, const V: u16> {
    associated_register: PhantomData<R>,
}

impl<R: RegisterLongName, const V: u16> ReadWriteRiscvCsr<R, V> {
    #[must_use]
    pub const fn new() -> Self {
        ReadWriteRiscvCsr {
            associated_register: PhantomData,
        }
    }
}

impl<R: RegisterLongName, const V: u16> Default for ReadWriteRiscvCsr<R, V> {
    fn default() -> Self {
        Self::new()
    }
}

// `Readable` and `Writeable` are not object-safe, so they cannot be
// provided as blanket implementations over `RiscvCsrInterface`; each
// accessor implements them individually instead.
impl<R: RegisterLongName, const V: u16> Readable for ReadWriteRiscvCsr<R, V> {
    type T = u64;
    type R = R;

    fn get(&self) -> u64 {
        self.get_value()
    }
}

impl<R: RegisterLongName, const V: u16> Writeable for ReadWriteRiscvCsr<R, V> {
    type T = u64;
    type R = R;

    fn set(&self, val_to_set: u64) {
        self.set_value(val_to_set);
    }
}

impl<R: RegisterLongName, const V: u16> RiscvCsrInterface for ReadWriteRiscvCsr<R, V> {
    type R = R;

    #[cfg(all(target_os = "none", target_arch = "riscv64"))]
    #[inline]
    fn atomic_replace(&self, val_to_set: u64) -> u64 {
        let prior: u64;
        // SAFETY: csrrw is an unprivileged instruction that atomically
        // swaps the CSR contents; it has no memory side effects beyond
        // the register itself.
        unsafe {
            asm!("csrrw {rd}, {csr}, {rs1}",
                 rd = out(reg) prior,
                 csr = const V,
                 rs1 = in(reg) val_to_set);
        }
        prior
    }

    // Host-side stub so the crate still compiles for unit testing.
    #[cfg(not(any(target_os = "none", target_arch = "riscv64")))]
    fn atomic_replace(&self, _value_to_set: u64) -> u64 {
        unimplemented!("RISC-V CSR {} Atomic Read/Write", V)
    }

    #[cfg(all(target_os = "none", target_arch = "riscv64"))]
    #[inline]
    fn get_value(&self) -> u64 {
        let fetched: u64;
        // SAFETY: csrr is a read-only CSR access with no memory side effects.
        unsafe {
            asm!("csrr {rd}, {csr}", rd = out(reg) fetched, csr = const V);
        }
        fetched
    }

    // Host-side stub so the crate still compiles for unit testing.
    #[cfg(not(any(target_os = "none", target_arch = "riscv64")))]
    fn get_value(&self) -> u64 {
        unimplemented!("reading RISC-V CSR {}", V)
    }

    #[cfg(all(target_os = "none", target_arch = "riscv64"))]
    #[inline]
    fn set_value(&self, val_to_set: u64) {
        // SAFETY: csrw is a write-only CSR access with no memory side effects.
        unsafe {
            asm!("csrw {csr}, {rs}", rs = in(reg) val_to_set, csr = const V);
        }
    }

    // Host-side stub so the crate still compiles for unit testing.
    #[cfg(not(any(target_os = "none", target_arch = "riscv64")))]
    fn set_value(&self, _val_to_set: u64) {
        unimplemented!("writing RISC-V CSR {}", V)
    }

    #[cfg(all(target_os = "none", target_arch = "riscv64"))]
    #[inline]
    fn read_and_set_bits(&self, bitmask: u64) -> u64 {
        let prior: u64;
        // SAFETY: csrrs atomically ORs the bitmask into the CSR and
        // returns the value observed before the modification.
        unsafe {
            asm!("csrrs {rd}, {csr}, {rs1}",
                 rd = out(reg) prior,
                 csr = const V,
                 rs1 = in(reg) bitmask);
        }
        prior
    }

    // Host-side stub so the crate still compiles for unit testing.
    #[cfg(not(any(target_os = "none", target_arch = "riscv64")))]
    fn read_and_set_bits(&self, bitmask: u64) -> u64 {
        unimplemented!(
            "RISC-V CSR {} Atomic Read and Set Bits, bitmask {:04x}",
            V,
            bitmask
        )
    }

    #[cfg(all(target_os = "none", target_arch = "riscv64"))]
    #[inline]
    fn read_and_clear_bits(&self, bitmask: u64) -> u64 {
        let prior: u64;
        // SAFETY: csrrc atomically clears the bitmask bits in the CSR
        // and returns the value observed before the modification.
        unsafe {
            asm!("csrrc {rd}, {csr}, {rs1}",
                 rd = out(reg) prior,
                 csr = const V,
                 rs1 = in(reg) bitmask);
        }
        prior
    }

    // Host-side stub so the crate still compiles for unit testing.
    #[cfg(not(any(target_os = "none", target_arch = "riscv64")))]
    fn read_and_clear_bits(&self, bitmask: u64) -> u64 {
        unimplemented!(
            "RISC-V CSR {} Atomic Read and Clear Bits, bitmask {:04x}",
            V,
            bitmask
        )
    }
}

/// A read/write CSR reached through an indirection window: the target
/// selector `V` is first stored into the address register `RA`, after
/// which the data register `RD` aliases the selected CSR.
///
/// # Safety invariant
///
/// The indirection operates correctly only if no concurrent code path
/// modifies the address register between the write to `RA` and the
/// access through `RD`. The TSM guarantees this by running with
/// interrupts disabled during CSR manipulation.
#[derive(Copy, Clone)]
pub struct IndirectReadWriteRiscvCsr<
    R: RegisterLongName,
    RA: RiscvCsrInterface,
    RD: RiscvCsrInterface,
    const V: u64,
> {
    associated_register: PhantomData<R>,
    address_register: RA,
    data_register: RD,
}

impl<R: RegisterLongName, RA: RiscvCsrInterface, RD: RiscvCsrInterface, const V: u64>
    IndirectReadWriteRiscvCsr<R, RA, RD, V>
{
    #[must_use]
    pub const fn new(ra: RA, rd: RD) -> Self {
        IndirectReadWriteRiscvCsr {
            associated_register: PhantomData,
            address_register: ra,
            data_register: rd,
        }
    }
}

// As above, `Readable`/`Writeable` are not object-safe and therefore
// have to be implemented directly on `IndirectReadWriteRiscvCsr`.
impl<R: RegisterLongName, RA: RiscvCsrInterface, RD: RiscvCsrInterface, const V: u64> Readable
    for IndirectReadWriteRiscvCsr<R, RA, RD, V>
{
    type T = u64;
    type R = R;

    fn get(&self) -> u64 {
        self.get_value()
    }
}

impl<R: RegisterLongName, RA: RiscvCsrInterface, RD: RiscvCsrInterface, const V: u64> Writeable
    for IndirectReadWriteRiscvCsr<R, RA, RD, V>
{
    type T = u64;
    type R = R;

    fn set(&self, val_to_set: u64) {
        self.set_value(val_to_set);
    }
}

impl<R: RegisterLongName, RA: RiscvCsrInterface, RD: RiscvCsrInterface, const V: u64>
    RiscvCsrInterface for IndirectReadWriteRiscvCsr<R, RA, RD, V>
{
    type R = R;

    #[inline]
    fn atomic_replace(&self, val_to_set: u64) -> u64 {
        self.address_register.set_value(V);
        self.data_register.atomic_replace(val_to_set)
    }

    #[inline]
    fn get_value(&self) -> u64 {
        self.address_register.set_value(V);
        self.data_register.get_value()
    }

    #[inline]
    fn set_value(&self, val_to_set: u64) {
        self.address_register.set_value(V);
        self.data_register.set_value(val_to_set)
    }

    #[inline]
    fn read_and_set_bits(&self, bitmask: u64) -> u64 {
        self.address_register.set_value(V);
        self.data_register.read_and_set_bits(bitmask)
    }

    #[inline]
    fn read_and_clear_bits(&self, bitmask: u64) -> u64 {
        self.address_register.set_value(V);
        self.data_register.read_and_clear_bits(bitmask)
    }
}
