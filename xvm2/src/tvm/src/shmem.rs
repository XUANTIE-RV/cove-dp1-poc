// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! Shared memory region for host–TSM vCPU state exchange.

use config::PAGE_SIZE;

const SCRATCH_OFFSET: usize = 0x0000;
const CSR_OFFSET: usize = 0x1000;

/// Number of CSR slots the CSR area holds (10-bit index space).
const CSR_SLOTS: usize = 1024;

/// Size of one virtual hart's shared memory region.
///
/// The SBI spec makes this a MUST, not a convention: "the size of the nested
/// acceleration shared memory must be 4096 + (1024 * (XLEN / 8)) bytes"
/// (riscv-sbi v3.0 §15). On RV64 that is 0x3000 — a 4KB scratch area followed by
/// an 8KB CSR area. Sizing a per-vCPU region any smaller makes the higher CSR
/// slots land outside it, which is exactly how the per-vCPU stride came to
/// overlap its neighbour and run off the end of the host's allocation.
pub const SHMEM_SIZE: usize = CSR_OFFSET + CSR_SLOTS * 8;

/// Whole 4KB pages one shared memory region spans. Used to size and stride the
/// per-vCPU area, so the two can never drift apart.
pub const SHMEM_PAGES_PER_VCPU: u64 = SHMEM_SIZE as u64 / PAGE_SIZE;

// The spec requires each region's base to be 4096-aligned. Striding by
// `SHMEM_SIZE` only preserves that if the size is a whole page count.
const _: () = assert!(SHMEM_SIZE as u64 % PAGE_SIZE == 0);
const _: () = assert!(SHMEM_PAGES_PER_VCPU == 3);
// Every reachable CSR slot must fit inside one region.
const _: () = assert!(CSR_OFFSET + (CSR_SLOTS - 1) * 8 + 8 <= SHMEM_SIZE);

// CSR numbers for shared memory indexing
pub const CSR_HTVAL_TRAP_VAL: u16 = 0x643;
pub const CSR_HTINST_TRAP_INST: u16 = 0x64A;
pub const CSR_SCAUSE_TRAP_CTRL: u16 = 0x142;
pub const CSR_STVAL_TRAP_VAL: u16 = 0x143;

/// Number of GPR slots the scratch area defines (x0..x31).
///
/// [`gpr_offset`] multiplies a raw index, so this bound is what keeps a GPR
/// access inside the scratch area. Without it an index of 512 or more lands at
/// offset 0x1000 and beyond, i.e. inside the CSR area, where it would overwrite
/// the slots the host reads `scause`/`stval`/`htval`/`htinst` from.
pub const SHMEM_GPR_SLOTS: usize = 32;

// The GPR array must stay wholly inside the scratch area.
const _: () = assert!(SCRATCH_OFFSET + SHMEM_GPR_SLOTS * 8 <= CSR_OFFSET);

/// Compute the byte offset of a CSR slot within the shared memory.
pub const fn csr_offset(csr: u16) -> usize {
    let index = (((csr as usize) & 0xC00) >> 2) | ((csr as usize) & 0xFF);
    CSR_OFFSET + index * 8
}

/// Compute the byte offset of a GPR slot within the scratch area.
///
/// Only meaningful for `reg < SHMEM_GPR_SLOTS`; the accessors enforce that.
pub const fn gpr_offset(reg: usize) -> usize {
    SCRATCH_OFFSET + reg * 8
}

/// Type-safe wrapper around one virtual hart's shared memory region.
///
/// Carries the region length so every access is bounds-checked. Without it a
/// mismatch between the per-vCPU stride and the actual region size is a silent
/// write past the end — which is precisely how the CSR area came to spill into
/// the neighbouring vCPU's region and off the end of the host's allocation.
pub struct Shmem {
    base: *mut u8,
    len: usize,
}

impl Shmem {
    /// # Safety
    /// Caller must ensure `base` points to a valid region of at least `len`
    /// bytes. Pass [`SHMEM_SIZE`] for a full spec-sized region.
    pub unsafe fn new(base: *mut u8, len: usize) -> Self {
        Self { base, len }
    }

    /// True if an 8-byte slot at `offset` lies wholly inside the region.
    #[inline]
    fn holds_slot(&self, offset: usize) -> bool {
        match offset.checked_add(8) {
            Some(end) => end <= self.len,
            None => false,
        }
    }

    /// Write a CSR slot. Returns `false` and writes nothing if the slot falls
    /// outside the region.
    ///
    /// # Safety
    ///
    /// Caller must ensure `self.base` points to a valid shared memory region
    /// of the length given at construction.
    #[inline]
    #[must_use]
    pub unsafe fn write_csr(&self, csr: u16, val: u64) -> bool {
        let offset = csr_offset(csr);
        if !self.holds_slot(offset) {
            return false;
        }
        core::ptr::write_unaligned(self.base.add(offset) as *mut u64, val);
        true
    }

    /// Read a CSR slot, or `None` if the slot falls outside the region.
    ///
    /// # Safety
    ///
    /// Caller must ensure `self.base` points to a valid shared memory region
    /// of the length given at construction.
    #[inline]
    #[must_use]
    pub unsafe fn read_csr(&self, csr: u16) -> Option<u64> {
        let offset = csr_offset(csr);
        if !self.holds_slot(offset) {
            return None;
        }
        Some(core::ptr::read_unaligned(
            self.base.add(offset) as *const u64
        ))
    }

    /// Write a GPR slot. Returns `false` and writes nothing if `reg` is not one
    /// of the [`SHMEM_GPR_SLOTS`] defined registers, or if the slot falls
    /// outside the region.
    ///
    /// The register-count bound is not redundant with the length check: the
    /// region is 0x3000 bytes in production, so length alone would accept any
    /// index up to 1535 and let a GPR write land in the CSR area.
    ///
    /// # Safety
    ///
    /// Caller must ensure `self.base` points to a valid shared memory region
    /// of the length given at construction.
    #[inline]
    #[must_use]
    pub unsafe fn write_gpr(&self, reg: usize, val: u64) -> bool {
        if reg >= SHMEM_GPR_SLOTS {
            return false;
        }
        let offset = gpr_offset(reg);
        if !self.holds_slot(offset) {
            return false;
        }
        core::ptr::write_unaligned(self.base.add(offset) as *mut u64, val);
        true
    }

    /// Read a GPR slot, or `None` if `reg` is not one of the
    /// [`SHMEM_GPR_SLOTS`] defined registers or the slot falls outside the
    /// region.
    ///
    /// # Safety
    ///
    /// Caller must ensure `self.base` points to a valid shared memory region
    /// of the length given at construction.
    #[inline]
    #[must_use]
    pub unsafe fn read_gpr(&self, reg: usize) -> Option<u64> {
        if reg >= SHMEM_GPR_SLOTS {
            return None;
        }
        let offset = gpr_offset(reg);
        if !self.holds_slot(offset) {
            return None;
        }
        Some(core::ptr::read_unaligned(
            self.base.add(offset) as *const u64
        ))
    }
}
