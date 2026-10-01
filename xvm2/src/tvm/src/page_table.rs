// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! Sv{39,48,57}x4 G-stage page table operations.

#[cfg(all(target_arch = "riscv64", target_os = "none"))]
use core::ptr;

#[cfg(all(target_arch = "riscv64", target_os = "none"))]
use utils::println;

#[cfg(all(target_arch = "riscv64", target_os = "none"))]
use super::tvm::get_current_tvm;

// Re-exported so existing `tvm::page_table::PAGE_SIZE` users keep working;
// the value itself is defined once in the `config` crate.
pub use config::PAGE_SIZE;

/// G-stage paging mode, distinguished by the number of page-table levels.
///
/// All three modes share a 16KB (4-page) root table; they differ only in how
/// many translation levels the walk traverses and how the root VPN is sliced.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u64)]
#[allow(dead_code)] // Sv39x4/Sv57x4 are selectable alternatives; default is Sv48x4.
pub enum GStageMode {
    /// Sv39x4: 3 levels, 39-bit GPA + 2-bit extension.
    ///
    /// Discriminant is 0 so a zero-initialized `Tvm` defaults to Sv39x4.
    Sv39x4 = 0,
    /// Sv48x4: 4 levels, 48-bit GPA + 2-bit extension.
    Sv48x4 = 1,
    /// Sv57x4: 5 levels, 57-bit GPA + 2-bit extension.
    Sv57x4 = 2,
}

impl GStageMode {
    /// Number of page-table levels traversed during a walk.
    pub const fn levels(self) -> usize {
        match self {
            GStageMode::Sv39x4 => 3,
            GStageMode::Sv48x4 => 4,
            GStageMode::Sv57x4 => 5,
        }
    }

    /// HGATP.MODE field value for this paging scheme.
    pub const fn hgatp_mode(self) -> u64 {
        match self {
            GStageMode::Sv39x4 => 8,
            GStageMode::Sv48x4 => 9,
            GStageMode::Sv57x4 => 10,
        }
    }

    /// Number of significant GPA bits this mode can translate.
    ///
    /// 12 bits of page offset, 9 bits per non-root level, and 11 at the root
    /// because the x4 root table spans four pages. Anything above this is not
    /// representable: `vpn_index` would mask the excess away and the walk would
    /// silently land on an unrelated entry.
    pub const fn gpa_bits(self) -> u32 {
        12 + 9 * (self.levels() as u32 - 1) + 11
    }

    /// Whether `gpa` fits in the address space this mode can translate.
    pub const fn gpa_in_range(self, gpa: u64) -> bool {
        let bits = self.gpa_bits();
        // `bits` is at most 59, so the shift cannot overflow.
        gpa >> bits == 0
    }
}

/// Sv39x4 PTE permission bits.
///
/// `PTE_VALID` and the R/W/X bits are not target-gated: [`pte_is_valid`],
/// [`pte_is_leaf`] and [`pte_hpa`] are available on the host too, so the checks
/// that consume them can be unit-tested.
const PTE_VALID: u64 = 1 << 0;
const PTE_READ: u64 = 1 << 1;
const PTE_WRITE: u64 = 1 << 2;
const PTE_EXEC: u64 = 1 << 3;
#[cfg(all(target_arch = "riscv64", target_os = "none"))]
const PTE_USER: u64 = 1 << 4;
#[cfg(all(target_arch = "riscv64", target_os = "none"))]
const PTE_ACCESS: u64 = 1 << 6;
#[cfg(all(target_arch = "riscv64", target_os = "none"))]
const PTE_DIRTY: u64 = 1 << 7;

/// Allocate a zeroed page from the page table pool.
///
/// Returns `None` when the pool is exhausted so the caller can report the
/// failure back to the host as an SBI error instead of the TSM aborting.
/// Previously the pool-exhausted branch called `panic!`, which turned a
/// host-triggerable condition (too few pages donated via
/// `AddTvmPageTablePages`) into a TSM crash.
///
/// # Safety
/// Atomically allocate a page from the pool.
/// Thread-safe: uses compare_exchange loop to prevent two harts from
/// getting the same page when concurrent IMSIC setup occurs.
#[cfg(all(target_arch = "riscv64", target_os = "none"))]
pub unsafe fn pgt_alloc_page() -> Option<u64> {
    use core::sync::atomic::{AtomicU64, Ordering};
    // Raw pointer, not `&mut Tvm`: this runs underneath callers that already
    // hold a `&mut Tvm` (e.g. `setup_vcpu_imsic` → `map_page`), so a second
    // `&mut` to the same static TVM here would alias. Touch the fields through
    // the pointer instead — all accesses below are reads or atomics.
    let tvm = get_current_tvm();
    // Use atomic CAS loop on pgt_pool_next to prevent races.
    // Safety: pgt_pool_next is only accessed through this function at runtime.
    let pool_next_ptr = ptr::addr_of!((*tvm).pgt_pool_next) as *const AtomicU64;
    let pool_next = &*pool_next_ptr;
    loop {
        let current = pool_next.load(Ordering::Acquire);
        let pool_end = ptr::addr_of!((*tvm).pgt_pool_end).read();
        if current >= pool_end {
            if (*tvm).claim_pgt_pool_report() {
                println!(
                    "[TSM] pgt_pool exhausted: next=0x{:x} end=0x{:x}",
                    current, pool_end
                );
            }
            return None;
        }
        let next = current + PAGE_SIZE;
        if pool_next
            .compare_exchange(current, next, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            ptr::write_bytes(current as *mut u8, 0, PAGE_SIZE as usize);
            return Some(current);
        }
        // CAS failed: another hart allocated, retry
    }
}

/// Mask covering the PPN field of a PTE (bits 53:10 → 44-bit PPN).
const PTE_PPN_MASK: u64 = 0xFFF_FFFF_FFFF;

/// Extract the VPN index for `level` from a GPA.
///
/// Levels 0..(top-1) use 9 bits each (512 entries). The root level (`top`)
/// uses 11 bits (2048 entries) because the x4 root spans 4 pages.
#[inline]
#[allow(dead_code)]
fn vpn_index(gpa: u64, level: usize, top_level: usize) -> u64 {
    let shift = 12 + 9 * level;
    let mask = if level == top_level { 0x7FF } else { 0x1FF };
    (gpa >> shift) & mask
}

/// Walk a G-stage page table and install a 4KB leaf mapping from GPA to HPA.
///
/// `mode` selects the number of levels (Sv39x4=3, Sv48x4=4, Sv57x4=5); all
/// modes share the same 16KB root table and per-level 9/11-bit VPN slicing.
///
/// Returns `true` when every intermediate table already exists or was
/// successfully allocated. Returns `false` if the pool ran out mid-walk;
/// the caller must report this back to the host, because the target GPA
/// is left with an incomplete mapping (guest access will fault). Partial
/// mappings from previous levels are not rolled back — the CAS-installed
/// pages become dead subtrees still linked into the walked table. Rollback
/// is deferred until a caller actually needs it.
///
/// Note: the pool `pgt_alloc_page` draws from is `get_current_tvm()`, not
/// the TVM that owns `root`. In `ConvertAiaImsic` those two identities can
/// diverge (that arm does not set `CURRENT_GUEST_ID`), so "the wasted
/// pages belong to this TVM" is not universally true — it holds on the
/// COVH arms that establish `current_guest_id` first, and not on the COVI
/// convert-IMSIC arm. That decoupling is a known gap; do not rely on this
/// walk to reason about page ownership.
///
/// # Safety
/// `root` must be the 16KB-aligned root of a live G-stage table for `mode`.
#[cfg(all(target_arch = "riscv64", target_os = "none"))]
pub unsafe fn map_page(mode: GStageMode, root: u64, gpa: u64, hpa: u64) -> bool {
    use core::sync::atomic::{AtomicU64, Ordering};
    let top_level = mode.levels() - 1;

    // Descend from the root down to (but not including) the leaf level,
    // allocating intermediate tables on demand (thread-safe via CAS).
    let mut table_addr = root;
    for level in (1..=top_level).rev() {
        let table = table_addr as *mut u64;
        let entry_ptr = table.add(vpn_index(gpa, level, top_level) as usize);
        let entry_atomic = &*(entry_ptr as *const AtomicU64);
        let mut entry_val = entry_atomic.load(Ordering::Acquire);
        if entry_val & PTE_VALID == 0 {
            // Allocate a new intermediate page table
            let Some(next_page) = pgt_alloc_page() else {
                return false;
            };
            let new_entry = ((next_page >> 12) << 10) | PTE_VALID;
            // CAS: if another hart already filled this entry, use theirs
            match entry_atomic.compare_exchange(
                entry_val,
                new_entry,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    entry_val = new_entry;
                }
                Err(existing) => {
                    // Another hart won the race; use their entry
                    // (the page we allocated is wasted but harmless)
                    entry_val = existing;
                }
            }
        }
        table_addr = ((entry_val >> 10) & PTE_PPN_MASK) << 12;
    }

    // Level 0: leaf entry granting full RWX permissions.
    let leaf_table = table_addr as *mut u64;
    let leaf_entry = leaf_table.add(vpn_index(gpa, 0, top_level) as usize);
    *leaf_entry = ((hpa >> 12) << 10)
        | PTE_VALID
        | PTE_READ
        | PTE_WRITE
        | PTE_EXEC
        | PTE_USER
        | PTE_ACCESS
        | PTE_DIRTY;
    true
}

/// Walk a G-stage page table down to the 4KB leaf entry for `gpa`.
///
/// Returns the leaf entry pointer once every intermediate table along the walk
/// exists. Use [`pte_is_valid`] on the pointed-to value to tell an
/// existing-but-unmapped leaf from a live one.
///
/// `None` means there is no 4KB leaf to point at, for one of three reasons:
/// `gpa` is wider than `mode` can translate, an intermediate table is absent, or
/// the walk met a large-page leaf part way down.
///
/// Unlike [`map_page`] this never allocates, so it is safe to call on a GPA the
/// host has not populated yet.
///
/// # Safety
/// `root` must be the 16KB-aligned root of a live G-stage table for `mode`.
#[cfg(all(target_arch = "riscv64", target_os = "none"))]
pub unsafe fn get_pte(mode: GStageMode, root: u64, gpa: u64) -> Option<*mut u64> {
    use core::sync::atomic::{AtomicU64, Ordering};
    if !mode.gpa_in_range(gpa) {
        return None;
    }
    let top_level = mode.levels() - 1;

    let mut table_addr = root;
    for level in (1..=top_level).rev() {
        let entry_ptr = (table_addr as *const u64).add(vpn_index(gpa, level, top_level) as usize);
        // Acquire matches the CAS release in `map_page`, so a table another hart
        // just published is observed with its zeroed contents.
        let entry_val = (*(entry_ptr as *const AtomicU64)).load(Ordering::Acquire);
        if !pte_is_valid(entry_val) {
            return None;
        }
        if pte_is_leaf(entry_val) {
            // A large-page mapping covers this GPA, so there is no 4KB leaf and
            // the PPN names the mapped frame rather than a next-level table.
            return None;
        }
        table_addr = ((entry_val >> 10) & PTE_PPN_MASK) << 12;
    }

    Some((table_addr as *mut u64).add(vpn_index(gpa, 0, top_level) as usize))
}

/// Whether a PTE value describes a live mapping.
#[inline]
pub const fn pte_is_valid(pte: u64) -> bool {
    pte & PTE_VALID != 0
}

/// Whether a PTE is a leaf, i.e. it maps a frame rather than naming a table.
///
/// RISC-V marks leaves by any of R/W/X being set, so this is what distinguishes a
/// large-page mapping at a non-zero level from a pointer to the next level.
#[inline]
pub const fn pte_is_leaf(pte: u64) -> bool {
    pte & (PTE_READ | PTE_WRITE | PTE_EXEC) != 0
}

/// Physical address of the page a leaf PTE maps, or `None` if it is not live.
///
/// The COVH calls that block, unblock and remove pages address them by GPA,
/// while page metadata is indexed by physical address, so the mapping has to be
/// translated before the metadata can be consulted.
#[inline]
pub const fn pte_hpa(pte: u64) -> Option<u64> {
    if !pte_is_valid(pte) {
        return None;
    }
    Some(((pte >> 10) & PTE_PPN_MASK) << 12)
}

/// Physical address a leaf PTE maps, **ignoring the valid bit**.
///
/// `block` invalidates a page by clearing `PTE_VALID` while keeping the PPN and
/// the R/W/X bits, so that TVM accesses fault while the mapping information
/// survives for a later `unblock`. `unblock` and `remove` therefore act on pages
/// whose leaf is still a leaf (R/W/X set) but no longer valid, and they need the
/// PPN to index page metadata — which [`pte_hpa`] refuses to give once the valid
/// bit is gone. An all-zero PTE (never mapped) has no R/W/X and returns `None`.
#[inline]
pub const fn pte_leaf_hpa(pte: u64) -> Option<u64> {
    if !pte_is_leaf(pte) {
        return None;
    }
    Some(((pte >> 10) & PTE_PPN_MASK) << 12)
}

/// Clear the leaf mapping for `gpa` and flush this hart's G-stage TLB.
///
/// Returns whether a live mapping was actually removed. Metadata bookkeeping
/// alone is not enough when a page leaves a TVM: the PTE has to go too.
///
/// **Only the calling hart's TLB is invalidated.** `hfence.gvma` is local by
/// definition, so a stale translation may survive on another hart until the
/// fence protocol lands and drives every hart through an invalidation. Callers
/// must not treat a successful return as proof that no hart can still reach the
/// frame.
///
/// The leaf is swapped atomically so the read of the old value and the clear
/// cannot be split, matching the release/acquire pairing that [`map_page`] and
/// [`get_pte`] use for intermediate entries.
///
/// # Safety
/// `root` must be the 16KB-aligned root of a live G-stage table for `mode`, and
/// the caller must be on the TSM's uninterruptible path.
#[cfg(all(target_arch = "riscv64", target_os = "none"))]
pub unsafe fn unmap_page(mode: GStageMode, root: u64, gpa: u64) -> bool {
    use core::sync::atomic::{AtomicU64, Ordering};
    let Some(leaf) = get_pte(mode, root, gpa) else {
        return false;
    };
    let previous = (*(leaf as *const AtomicU64)).swap(0, Ordering::AcqRel);
    riscv_regs::hfence_gvma!();
    pte_is_valid(previous)
}

/// Invalidate the leaf for `gpa` without discarding it, and flush this hart's
/// G-stage TLB. Returns the mapped `hpa`, or `None` if there is no leaf.
///
/// Clears only `PTE_VALID`, leaving the PPN and the R/W/X bits in place. A TVM
/// access then takes a G-stage page fault, but the mapping is not forgotten:
/// [`revalidate_leaf`] can restore it, and [`pte_leaf_hpa`] can still recover the
/// PPN. This is what `TvmInvalidatePages` needs — the spec has it stop TVM access to
/// the page while keeping it removable and restorable, distinct from
/// [`unmap_page`], which drops the mapping entirely on `TvmRemovePages`.
///
/// Same single-hart caveat as [`unmap_page`]: `hfence.gvma` is local, so a stale
/// translation may persist on another hart until the fence protocol lands.
///
/// # Safety
/// `root` must be the 16KB-aligned root of a live G-stage table for `mode`, and
/// the caller must be on the TSM's uninterruptible path.
#[cfg(all(target_arch = "riscv64", target_os = "none"))]
pub unsafe fn invalidate_leaf(mode: GStageMode, root: u64, gpa: u64) -> Option<u64> {
    use core::sync::atomic::{AtomicU64, Ordering};
    let leaf = get_pte(mode, root, gpa)?;
    let previous = (*(leaf as *const AtomicU64)).fetch_and(!PTE_VALID, Ordering::AcqRel);
    riscv_regs::hfence_gvma!();
    pte_leaf_hpa(previous)
}

/// Restore a leaf previously cleared by [`invalidate_leaf`], and flush this
/// hart's G-stage TLB. Returns the mapped `hpa`, or `None` if there is no leaf.
///
/// Sets `PTE_VALID` back on, which re-admits TVM access to the page it still
/// points at. This is `TvmValidatePages`: it reverses a block rather than building
/// a new mapping, so it relies on the PPN and permission bits that
/// [`invalidate_leaf`] deliberately left behind.
///
/// # Safety
/// As [`invalidate_leaf`].
#[cfg(all(target_arch = "riscv64", target_os = "none"))]
pub unsafe fn revalidate_leaf(mode: GStageMode, root: u64, gpa: u64) -> Option<u64> {
    use core::sync::atomic::{AtomicU64, Ordering};
    let leaf = get_pte(mode, root, gpa)?;
    let previous = (*(leaf as *const AtomicU64)).fetch_or(PTE_VALID, Ordering::AcqRel);
    riscv_regs::hfence_gvma!();
    pte_leaf_hpa(previous)
}
