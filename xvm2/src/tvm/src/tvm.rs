// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! TVM (Trusted Virtual Machine) global state management.

use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use core::{mem, ptr};

#[cfg(all(target_arch = "riscv64", target_os = "none"))]
use super::page_meta;
use super::page_table::GStageMode;
use config::{
    MAX_HARTS, MAX_MEM_REGIONS, MAX_MMIO_REGIONS, MAX_SHARED_REGIONS, MAX_VCPUS, PAGE_SIZE,
};
use riscv_regs::VmCpuRegisters;
use utils::print::*;
use utils::sync::Mutex;
use vcpu::{Vcpu, VcpuStatus};

/// TVM lifecycle state as defined by the CoVE spec.
///
/// Discriminants intentionally start at 1 rather than matching the spec's
/// `TVM_INITIALIZING = 0` / `TVM_RUNNABLE = 1`: the static TVM instance is
/// zero-initialized, and an all-zero bit pattern must mean "unused".
/// With `Option<TvmState>` the zero pattern lands on the niche and reads
/// back as `None`, which would be impossible if a state owned value 0.
/// The spec's numeric values carry no ABI meaning today — no COVH call
/// reports TVM state back to the host.
#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TvmState {
    /// spec `TVM_INITIALIZING`: created, not yet runnable.
    Initializing = 1,
    /// spec `TVM_RUNNABLE`: finalized and allowed to run vCPUs.
    Runnable = 2,
}

/// Error type for TVM lifecycle operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TvmError {
    /// The single TVM slot already holds a live TVM; no slot is free for CreateTvm.
    NoFreeSlot,
    /// The G-stage root or shared memory pages the host supplied are not in a
    /// state that allows assigning them to this TVM.
    ControlPagesUnavailable,
    /// The declared memory region is malformed (zero length, unaligned, or it
    /// wraps), overlaps one already declared, or there is no free table entry.
    BadMemoryRegion,
}

/// One guest memory region declared by the host via `AddTvmMemoryRegion`, or one
/// MMIO window declared by the guest via COVG `AddMmioRegion`.
///
/// `size == 0` marks an unused entry so the table is valid when the enclosing
/// `Tvm` is zero-initialized.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct MemRegion {
    pub gpa: u64,
    pub size: u64,
}

impl MemRegion {
    /// True if this entry describes a real region rather than a free slot.
    pub const fn is_used(&self) -> bool {
        self.size != 0
    }

    /// True if `gpa` falls inside this region. Always false for a free slot.
    pub const fn contains(&self, gpa: u64) -> bool {
        // `validate_region` rejects ranges that wrap, so this cannot overflow for
        // a used entry.
        self.size != 0 && gpa >= self.gpa && gpa - self.gpa < self.size
    }

    /// True if this region shares any address with `[gpa, gpa + size)`.
    const fn overlaps(&self, gpa: u64, size: u64) -> bool {
        if self.size == 0 {
            return false;
        }
        // Half-open intervals overlap unless one ends at or before the other starts.
        gpa < self.gpa + self.size && self.gpa < gpa + size
    }
}

/// Reject a range the TSM must not store.
///
/// Every parameter originates outside the TSM. A zero length would also make a
/// used entry indistinguishable from a free slot, and a wrapping range would break
/// the interval arithmetic in [`MemRegion::contains`] and [`MemRegion::overlaps`].
fn validate_region(gpa: u64, size: u64) -> Result<(), TvmError> {
    if size == 0 || gpa % PAGE_SIZE != 0 || size % PAGE_SIZE != 0 {
        return Err(TvmError::BadMemoryRegion);
    }
    if gpa.checked_add(size).is_none() {
        return Err(TvmError::BadMemoryRegion);
    }
    Ok(())
}

/// Insert `[gpa, gpa + size)` into a region table, refusing any overlap.
///
/// Overlap is refused for both tables, for two different reasons that happen to
/// agree. KVM memslots do not overlap, so a RAM declaration that does is a host
/// bug. And for MMIO the spec says so outright: "the region must not overlap with
/// a previously defined region" (COVG `AddMmioRegion`).
fn region_insert(table: &mut [MemRegion], gpa: u64, size: u64) -> Result<(), TvmError> {
    validate_region(gpa, size)?;
    if table.iter().any(|r| r.overlaps(gpa, size)) {
        return Err(TvmError::BadMemoryRegion);
    }
    match table.iter_mut().find(|r| !r.is_used()) {
        Some(slot) => {
            *slot = MemRegion { gpa, size };
            Ok(())
        }
        None => Err(TvmError::BadMemoryRegion),
    }
}

/// Insert `[gpa, gpa + size)` merging any overlapping or abutting entries.
///
/// Shared-memory semantics differ from the other two tables: the guest shares
/// contiguous DMA ranges page by page (`set_memory_decrypted` is called once
/// per buffer by swiotlb and every virtio driver), so overlap means "shared
/// again" and abutment means "same pool grown" — both merge into one span
/// rather than refuse. Measured boot floor was 9 separate declarations of
/// which 5 were pairwise contiguous; without merging the table overflows.
///
/// The table invariant this maintains: no two used entries overlap or abut,
/// so any contiguously-covered range is covered by exactly one entry
/// ([`region_covers_range`] relies on that).
fn region_insert_coalescing(table: &mut [MemRegion], gpa: u64, size: u64) -> Result<(), TvmError> {
    validate_region(gpa, size)?;
    let mut start = gpa;
    let mut end = gpa + size;
    // Absorb every entry the growing span touches. One pass suffices: the
    // span only grows to the union of the new range and entries it already
    // touched, and existing entries never touch each other (invariant), so an
    // entry not touching the union was not touched by any part of it.
    for slot in table.iter_mut() {
        if slot.is_used() && slot.gpa <= end && start <= slot.gpa + slot.size {
            start = start.min(slot.gpa);
            end = end.max(slot.gpa + slot.size);
            *slot = MemRegion::default();
        }
    }
    match table.iter_mut().find(|r| !r.is_used()) {
        Some(slot) => {
            *slot = MemRegion {
                gpa: start,
                size: end - start,
            };
            Ok(())
        }
        // Unreachable in practice: absorbing frees at least one slot whenever
        // the new range touches anything, so this is only hit when the table
        // is full of mutually disjoint entries and the new range touches none.
        None => Err(TvmError::BadMemoryRegion),
    }
}

/// Remove exactly `[gpa, gpa + size)`, splitting entries that extend past it.
///
/// Unlike the MMIO overlap-delete, unsharing must not take neighbours with it:
/// the guest re-encrypts one buffer while the rest of a merged span stays
/// shared, and dropping the whole entry would make the consumer gate refuse
/// the host's later legitimate `AddTvmSharedPages` there. So overlapped
/// entries are trimmed to their remainders outside the hole.
///
/// At most one entry can need both a left and a right remainder (entries are
/// disjoint, so only a strict superset of the hole does), which costs one free
/// slot. That is checked before anything is mutated: the table is never left
/// half-trimmed.
///
/// Removing nothing is an error, same rationale as [`region_remove_overlapping`].
fn region_remove_splitting(
    table: &mut [MemRegion],
    gpa: u64,
    size: u64,
) -> Result<usize, TvmError> {
    validate_region(gpa, size)?;
    let end = gpa + size;
    let needs_both = |r: &MemRegion| r.gpa < gpa && end < r.gpa + r.size;
    if table
        .iter()
        .any(|r| r.is_used() && r.overlaps(gpa, size) && needs_both(r))
        && !table.iter().any(|r| !r.is_used())
    {
        return Err(TvmError::BadMemoryRegion);
    }
    let mut removed = 0;
    let mut right: Option<MemRegion> = None;
    for slot in table.iter_mut() {
        if !slot.overlaps(gpa, size) {
            continue;
        }
        removed += 1;
        let slot_end = slot.gpa + slot.size;
        let left = (slot.gpa < gpa).then_some(MemRegion {
            gpa: slot.gpa,
            size: gpa - slot.gpa,
        });
        if end < slot_end {
            // Entries are disjoint, so at most one reaches past the hole.
            right = Some(MemRegion {
                gpa: end,
                size: slot_end - end,
            });
        }
        *slot = left.unwrap_or_default();
    }
    if removed == 0 {
        return Err(TvmError::BadMemoryRegion);
    }
    if let Some(r) = right {
        // The free slot was reserved above (or freed by this very trim).
        *table
            .iter_mut()
            .find(|r| !r.is_used())
            .expect("free slot checked before mutation") = r;
    }
    Ok(removed)
}

/// True if `[gpa, gpa + size)` is entirely inside one used entry.
///
/// Sufficient for full coverage because both shared-table mutators keep used
/// entries non-overlapping and non-abutting: contiguous coverage can never be
/// stitched together from two entries.
fn region_covers_range(table: &[MemRegion], gpa: u64, size: u64) -> bool {
    if size == 0 || gpa.checked_add(size).is_none() {
        return false;
    }
    table
        .iter()
        .any(|r| r.contains(gpa) && gpa + size <= r.gpa + r.size)
}

/// Drop every entry overlapping `[gpa, gpa + size)`, and report how many went.
///
/// The spec is explicit that this is not an exact-match delete: "The TSM must
/// remove every MMIO region that overlaps with the requested range" (COVG
/// `RemoveMmioRegion`). So a request may clear several entries, and may clear an
/// entry reaching beyond the requested range — after which accesses there fault,
/// which is the stated intent.
///
/// Removing nothing is reported as an error: the caller asked to stop trapping a
/// range that was never declared, and silently succeeding would tell the guest its
/// window is gone when no window existed.
fn region_remove_overlapping(
    table: &mut [MemRegion],
    gpa: u64,
    size: u64,
) -> Result<usize, TvmError> {
    validate_region(gpa, size)?;
    let mut removed = 0;
    for slot in table.iter_mut() {
        if slot.overlaps(gpa, size) {
            *slot = MemRegion::default();
            removed += 1;
        }
    }
    if removed == 0 {
        return Err(TvmError::BadMemoryRegion);
    }
    Ok(removed)
}

/// True if `gpa` falls in any used entry of `table`.
fn region_contains(table: &[MemRegion], gpa: u64) -> bool {
    table.iter().any(|r| r.contains(gpa))
}

/// The two region tables a TVM keeps, held together under one lock.
///
/// They are locked together rather than separately because the exit path asks
/// both in one breath ("is this guest RAM, or a declared device window?") and
/// because a declaration must be checked against both: a RAM region overlapping
/// a live MMIO window would silently demote that window's accesses to demand
/// paging.
///
/// An entry with `size == 0` is unused, which keeps both tables valid when the
/// whole `Tvm` is zero-initialized. Insertion refuses a zero length, so an
/// in-use entry can never look empty.
pub struct Regions {
    /// Guest RAM, declared by the host via `AddTvmMemoryRegion`, one per memslot.
    ///
    /// The exit path asks this table whether a faulting GPA is guest RAM (demand
    /// paging) or outside it, and the host classifies the same fault against
    /// *all* of its memslots — so every declared region has to be kept, not just
    /// the most recent one.
    pub mem: [MemRegion; MAX_MEM_REGIONS],
    /// MMIO windows, declared by the *guest* via COVG `AddMmioRegion`.
    ///
    /// This is what lets the fault path honour the spec's rule that the TSM may
    /// read or write the guest registers involved in a fault only when the access
    /// is inside an emulated MMIO region. Without it the TSM had to infer "not
    /// guest RAM, so presumably a device", which grants that access to any
    /// address the guest never declared.
    pub mmio: [MemRegion; MAX_MMIO_REGIONS],
    /// Shared-memory regions the *guest* has declared via COVG `ShareMemory`.
    ///
    /// spec §10.15 requires `AddTvmSharedPages` (COVH FID #13) to check that the
    /// target `tvm_base_page_address` lies within a region the TVM has already
    /// asked to be shared. This is the table that decides — kept per-TVM because
    /// the declaration is scoped to the TVM's own guest interface. Registered on
    /// `ShareMemory`, removed on `UnshareMemory`, both before the exit to the
    /// host so `AddTvmSharedPages` from the host can look it up immediately
    /// after the resume.
    pub shared: [MemRegion; MAX_SHARED_REGIONS],
}

impl Default for Regions {
    fn default() -> Self {
        Regions {
            mem: [MemRegion::default(); MAX_MEM_REGIONS],
            mmio: [MemRegion::default(); MAX_MMIO_REGIONS],
            shared: [MemRegion::default(); MAX_SHARED_REGIONS],
        }
    }
}

/// TVM-global state shared across all vCPUs, plus per-vCPU register contexts.
pub struct Tvm {
    /// `None` when no TVM is active; the host sees such a guest_id as
    /// invalid. `Some` carries the spec lifecycle state.
    pub state: Option<TvmState>,
    /// G-stage paging mode (Sv39x4 / Sv48x4 / Sv57x4).
    ///
    /// Zero-initialized to Sv39x4, but `CreateTvm` overrides it to Sv48x4
    /// (the current default) before any mapping happens.
    pub pgt_mode: GStageMode,
    /// Root of G-stage page table (physical address, 16KB-aligned)
    pub pgt_root_addr: u64,
    /// Pool of page table pages from AddTvmPageTablePages
    pub pgt_pool_next: u64,
    pub pgt_pool_end: u64,
    /// Entry point and argument from FinalizeTvm
    pub entry_sepc: u64,
    pub entry_arg: u64,
    /// Guest RAM and guest-declared MMIO windows, behind one lock.
    ///
    /// # Concurrency
    ///
    /// Both tables are written from the COVG declaration path (`ioremap`) and
    /// read from the exit path (a guest fault). On this single-hart POC those
    /// never overlap in time, but the lock still guards the tables so the
    /// scan-and-insert of a new region stays a single atomic step and a reader
    /// never observes a half-written `MemRegion`.
    ///
    /// Reader tearing is the dangerous case the lock rules out: `MemRegion` is
    /// two independent 64-bit fields, and a reader that saw `size` land before
    /// `gpa` would read `{gpa: 0, size: N}` and answer "declared MMIO" for
    /// `[0, N)`, an address range the guest never declared. That reopens
    /// exactly the fail-open hole this table exists to close.
    ///
    /// The critical sections are a bounded scan of 8 + 16 entries with no I/O in
    /// them — in particular no `println!`, so this lock is never held while
    /// waiting on the console.
    pub regions: Mutex<Regions>,
    /// How many undeclared-fault diagnostics this TVM has already printed.
    ///
    /// Per-TVM rather than global: a single global budget is spent by whichever
    /// TVM faults first, after which every other TVM's undeclared access is
    /// silent — which is the very "looks like a hang with no error line" case the
    /// diagnostic exists to make visible. Reset on destroy.
    pub undeclared_faults: AtomicUsize,
    /// How many refused-MMIO-declaration diagnostics this TVM has printed.
    ///
    /// Separate budget from [`Self::undeclared_faults`] so a chatty declaration
    /// path cannot spend the exit path's diagnostics. Bounded because the guest
    /// drives this call: a repeat declaration of a page already covered is refused
    /// by design (sub-page PCI BARs make that routine), and the guest may repeat it
    /// indefinitely.
    pub mmio_refusals: AtomicUsize,
    /// How many refused-shared-region diagnostics this TVM has printed.
    ///
    /// Independent budget: reusing the MMIO refusal counter during the option-A
    /// diag round let four benign shared-region lines silence a later real MMIO
    /// refusal. Guest-driven, so bounded for the same println-panic reason as
    /// the other budgets.
    pub shared_refusals: AtomicUsize,
    /// How many refused-`AddTvmSharedPages` diagnostics this TVM has printed.
    ///
    /// Host-driven and kept apart from [`Self::shared_refusals`] so a guest
    /// spending its own budget on benign declaration refusals cannot silence
    /// the report of a host mapping shared pages into an undeclared GPA.
    pub host_shared_refusals: AtomicUsize,
    /// How many `pgt_pool exhausted` diagnostics this TVM has printed.
    ///
    /// `ConvertAiaImsic` loops over vCPUs and can trigger many failed
    /// allocations in a single call. Without a cap the host could keep the
    /// TSM console busy by repeatedly running that FID after donating an
    /// undersized pool.
    pub pgt_pool_reports: AtomicUsize,
    /// Shared memory base address (from CreateTvm's tvm_state_addr).
    /// KVM reads exit info from this region after RunTvmVcpu.
    pub shmem_addr: u64,
    /// Per-hart IMSIC hardware addresses from ConvertAiaImsic.
    /// Indexed by hart_id, separate from vcpu fields to avoid corruption.
    ///
    /// Data flow (three steps):
    /// 1. ConvertAiaImsic writes the HPA into this per-hart table.
    /// 2. On first RunTvmVcpu, `Tvm::setup_vcpu_imsic` copies the entry for
    ///    the actual execution hart into the per-vCPU cache
    ///    (`Vcpu::imsic_hw_addr`).
    /// 3. InjectTvmCpu only reads the per-vCPU cache.
    ///
    /// Two layers are needed because ConvertImsic is hart-scoped (the HPA
    /// source arrives per hart, before the vCPU→hart binding is known),
    /// while the per-vCPU cache spares the injection hot path a per-hart
    /// table lookup.
    pub imsic_hw_by_hart: [u64; MAX_HARTS],
    pub vcpus: [Vcpu; MAX_VCPUS],
}

impl Tvm {
    /// Get a mutable reference to the current vCPU's register state.
    ///
    /// # Safety
    /// Caller must have set `CURRENT_VCPU_ID` for this hart via `set_current_vcpu_id`.
    #[inline(always)]
    pub unsafe fn vcpu_regs(&mut self) -> &mut VmCpuRegisters {
        &mut self.vcpus[current_vcpu_id()].regs
    }

    /// Get the current vCPU's `VcpuState`.
    ///
    /// # Safety
    /// Caller must have set `CURRENT_VCPU_ID` for this hart via
    /// [`set_current_vcpu_id`]; the stored index must be in bounds.
    #[inline(always)]
    pub unsafe fn current_vcpu(&mut self) -> &mut Vcpu {
        &mut self.vcpus[current_vcpu_id()]
    }

    /// Map the IMSIC for a vcpu during first RunTvmVcpu.
    /// Uses the actual execution hart to look up the correct IMSIC HPA.
    ///
    /// # Safety
    /// Caller must have exclusive access to this TVM's state (`&mut self`)
    /// and pass the hart id of the actual execution hart.
    #[cfg(all(target_arch = "riscv64", target_os = "none"))]
    pub unsafe fn setup_vcpu_imsic(&mut self, vcpu_idx: usize, hart: usize) -> bool {
        if vcpu_idx >= MAX_VCPUS || hart >= MAX_HARTS {
            return false;
        }
        let gpa = self.vcpus[vcpu_idx].imsic_gpa;
        let hpa = self.imsic_hw_by_hart[hart];

        // Always record actual pcpu so ConvertImsic can find pending vcpus
        self.vcpus[vcpu_idx].pcpu_id = hart;

        if gpa != 0 && hpa != 0 {
            self.vcpus[vcpu_idx].imsic_hw_addr = hpa;
            if !crate::page_table::map_page(self.pgt_mode, self.pgt_root_addr, gpa, hpa) {
                println!(
                    "[TSM] REFUSE setup_vcpu_imsic: vcpu={} pool-exhausted while mapping IMSIC",
                    vcpu_idx
                );
                return false;
            }
            core::arch::asm!("sfence.vma zero, zero");
            riscv_regs::hfence_gvma!();
        }
        true
    }

    /// Allocate the single TVM instance and (re)initialize it from `params`.
    ///
    /// Zeroes the whole `Tvm` instance (preserving the per-hart IMSIC source
    /// table populated by ConvertAiaImsic across destroy/create cycles),
    /// marks it `Initializing`, records the page-directory root and
    /// shared-memory base, and zero-fills both host-provided regions.
    ///
    /// Returns `Err(NoFreeSlot)` when the single TVM slot already holds a live TVM.
    ///
    /// # Safety
    /// - Must be called from a single hart at a time (the free-slot scan
    ///   and slot initialization on the static `TVM` instance are not atomic).
    /// - `params` must already have passed the caller's alignment/non-null
    ///   validation (`tvm_page_directory_addr` 16 KiB-aligned,
    ///   `tvm_state_addr` page-aligned, both non-null).
    /// - The caller must guarantee `tvm_page_directory_addr` (4 pages) and
    ///   `tvm_state_addr` (`MAX_VCPUS * SHMEM_PAGES_PER_VCPU` pages, i.e. one
    ///   full shared memory region per vCPU) point to writable non-confidential
    ///   memory. Both extents are additionally checked against the tracked RAM
    ///   window before anything is written to them, so a host that supplies a
    ///   short or bogus region gets the create refused rather than having the
    ///   TSM write past its allocation.
    pub unsafe fn create(params: &sbi_rt::TvmCreateParams) -> Result<u64, TvmError> {
        let gid = match find_free_tvm_slot() {
            Some(gid) => gid,
            None => return Err(TvmError::NoFreeSlot),
        };
        // Snapshot the current generation before publishing the TVM:
        // the handle handed back to the host must match what verify_tvm_handle
        // will read until the next destroy bumps it.
        let generation = GENERATION_COUNTER.load(Ordering::Acquire);
        let tvm = &mut *get_tvm_by_id(gid);
        let saved_imsic_hw = tvm.imsic_hw_by_hart;
        ptr::write_bytes(tvm as *mut _ as *mut u8, 0, mem::size_of::<Tvm>());
        tvm.imsic_hw_by_hart = saved_imsic_hw;
        tvm.state = Some(TvmState::Initializing);
        tvm.pgt_mode = GStageMode::Sv48x4;
        tvm.pgt_root_addr = params.tvm_page_directory_addr;
        tvm.shmem_addr = params.tvm_state_addr;

        // Both of these belong to this TVM and must be tracked, otherwise
        // `is_claimed()` reports false for them and the host can reclaim a live
        // TVM's G-stage root or its shared memory pages.
        if Self::register_control_pages(gid, tvm.pgt_root_addr, tvm.shmem_addr).is_err() {
            Self::release_owned_pages(gid);
            tvm.state = None;
            return Err(TvmError::ControlPagesUnavailable);
        }

        ptr::write_bytes(tvm.pgt_root_addr as *mut u8, 0, 4 * PAGE_SIZE as usize);
        ptr::write_bytes(
            tvm.shmem_addr as *mut u8,
            0,
            (MAX_VCPUS as u64 * crate::shmem::SHMEM_PAGES_PER_VCPU * PAGE_SIZE) as usize,
        );
        Ok(encode_tvm_handle(generation))
    }

    /// Tear down this TVM: fully reset all vCPUs and reset the per-TVM
    /// lifecycle fields to their zero (unused) values, releasing the instance
    /// (`state = None`) for reuse by a later CreateTvm.
    ///
    /// `imsic_hw_by_hart` is deliberately NOT cleared: the per-hart IMSIC
    /// source table from ConvertAiaImsic is preserved across destroy/create
    /// cycles.
    ///
    /// # Safety
    /// The caller must have verified this TVM is live and that
    /// none of its vCPUs is currently executing (`VcpuStatus::Running`).
    /// - `vcpu.reset()` internally uses `Ordering::Relaxed` for atomic stores;
    ///   this is safe because the caller guarantees no other hart is concurrently
    ///   accessing these vCPUs (see above "no vCPU running" precondition).
    pub unsafe fn destroy(&mut self, guest_id: usize) {
        for vcpu in self.vcpus.iter_mut() {
            vcpu.reset();
        }
        Self::release_owned_pages(guest_id);
        self.pgt_root_addr = 0;
        self.pgt_pool_next = 0;
        self.pgt_pool_end = 0;
        *self.regions.lock() = Regions::default();
        // Give the next TVM a full diagnostic budget.
        self.undeclared_faults.store(0, Ordering::Relaxed);
        self.mmio_refusals.store(0, Ordering::Relaxed);
        self.shared_refusals.store(0, Ordering::Relaxed);
        self.host_shared_refusals.store(0, Ordering::Relaxed);
        self.pgt_pool_reports.store(0, Ordering::Relaxed);
        self.shmem_addr = 0;
        self.state = None;
        // Retire every handle minted for this TVM's previous incarnation: the
        // next CreateTvm snapshots the bumped value, so a stale
        // handle can never validate again. The handle encodes the low 64 bits
        // of the u64 counter, so recycling a generation value would take 2^64
        // destroys — unreachable in practice.
        GENERATION_COUNTER.fetch_add(1, Ordering::Release);
    }

    /// Refuse a GPA range that already carries live mappings.
    ///
    /// `reserve_pages` only validates the physical side, so without this the host
    /// could point the same `guest_addr` at a second `page_addr`: the leaf PTE
    /// would be overwritten while the first frame stayed `ConfAssigned` under this
    /// TVM. Since block/remove are addressed by GPA, that frame could then never
    /// be torn down nor reclaimed.
    ///
    /// # Safety
    /// Caller must be on the TSM's uninterruptible path.
    #[cfg(all(target_arch = "riscv64", target_os = "none"))]
    unsafe fn reject_existing_mappings(
        &self,
        guest_addr: u64,
        num_pages: u64,
    ) -> Result<(), page_meta::TransitionError> {
        for i in 0..num_pages {
            let gpa = guest_addr + i * PAGE_SIZE;
            if let Some(leaf) = crate::page_table::get_pte(self.pgt_mode, self.pgt_root_addr, gpa) {
                if crate::page_table::pte_is_valid(*leaf) {
                    println!(
                        "[TSM] page_meta REFUSE map: gpa=0x{:x} already mapped (hpa=0x{:x})",
                        gpa,
                        crate::page_table::pte_hpa(*leaf).unwrap_or(0)
                    );
                    return Err(page_meta::TransitionError::WrongState);
                }
            }
        }
        Ok(())
    }

    /// Track this TVM's control pages so the host cannot reclaim them while it
    /// is alive.
    ///
    /// The two kinds differ: the G-stage root is confidential and TSM-private,
    /// but the shared memory pages are the host's window into the TVM and the
    /// host converts them with the shared funcid precisely so it keeps access.
    /// Marking them confidential would both contradict the MPT and make creation
    /// fail. Either way they end up owned, and `is_claimed()` covers both
    /// states, so reclaim refuses them until the TVM is destroyed.
    ///
    /// Both regions are first required to be tracked RAM. The caller zero-fills
    /// them as soon as this returns `Ok`, so this check is what stands between a
    /// host-supplied address and the TSM writing tens of kilobytes to it. The
    /// shared reservation cannot carry that weight by itself: `reserve_pages`
    /// tolerates untracked pages for `AddShared` because sharing device memory is
    /// legal, so a short or bogus region would be skipped rather than refused.
    #[cfg(all(target_arch = "riscv64", target_os = "none"))]
    unsafe fn register_control_pages(
        guest_id: usize,
        root_addr: u64,
        shmem_addr: u64,
    ) -> Result<(), page_meta::TransitionError> {
        let owner = page_meta::owner_of_guest(guest_id as u8);
        let shmem_pages = MAX_VCPUS as u64 * crate::shmem::SHMEM_PAGES_PER_VCPU;

        page_meta::require_tracked_range(root_addr, 4)?;
        page_meta::require_tracked_range(shmem_addr, shmem_pages)?;

        Self::reserve_pages(root_addr, 4, page_meta::PageOp::AddPageTable, owner)?;
        Self::reserve_pages(shmem_addr, shmem_pages, page_meta::PageOp::AddShared, owner)
    }

    /// Non-riscv64 fallback: no metadata table, so there is nothing to track.
    #[cfg(not(all(target_arch = "riscv64", target_os = "none")))]
    unsafe fn register_control_pages(
        _guest_id: usize,
        _root_addr: u64,
        _shmem_addr: u64,
    ) -> Result<(), ()> {
        Ok(())
    }

    /// Hand every page this TVM held back to the confidential-unassigned pool.
    ///
    /// Deliberately does *not* scrub here. Every route out of `ConfUnassigned`
    /// already wipes the frame before anyone can read it: `add_zero_pages`
    /// zeroes, `add_measured_pages` overwrites the full page, `pgt_alloc_page`
    /// zeroes each pool page as it hands it out, and `ReclaimPages` scrubs
    /// before the host regains access. Scrubbing again on this path would double
    /// the teardown cost of a large TVM for no added guarantee.
    ///
    /// Non-riscv builds have no metadata table, so this is a no-op there.
    #[cfg(all(target_arch = "riscv64", target_os = "none"))]
    unsafe fn release_owned_pages(guest_id: usize) {
        if guest_id != 0 {
            return;
        }
        let owner = page_meta::owner_of_guest(guest_id as u8);
        let mut table = page_meta::lock_table();
        for meta in table.iter_mut() {
            if let Some(next) = page_meta::release_for_owner(*meta, owner) {
                *meta = next;
            }
        }
    }

    #[cfg(not(all(target_arch = "riscv64", target_os = "none")))]
    unsafe fn release_owned_pages(_guest_id: usize) {}

    /// Validate then commit a page-state transition across a batch of pages.
    ///
    /// Two passes on purpose: if any page in the range is in the wrong state the
    /// whole batch is refused before a single mapping is created, so the host
    /// never has to unwind a half-applied request.
    ///
    /// Addresses outside the tracked RAM window are device memory (IMSIC and
    /// friends). Those carry no metadata, so only sharing them is permitted;
    /// converting or assigning device memory as confidential is refused.
    ///
    /// # Safety
    /// Caller must be on the TSM's uninterruptible path so the metadata table is
    /// not observed mid-update.
    #[cfg(all(target_arch = "riscv64", target_os = "none"))]
    unsafe fn reserve_pages(
        first_pa: u64,
        num_pages: u64,
        op: page_meta::PageOp,
        owner: u8,
    ) -> Result<(), page_meta::TransitionError> {
        let device_ok = matches!(op, page_meta::PageOp::AddShared);
        page_meta::validate_range(first_pa, num_pages)?;

        // One lock across validate-and-commit: two harts must not both pass
        // their own validation and then both commit the same page.
        let mut table = page_meta::lock_table();

        for i in 0..num_pages {
            let pa = first_pa + i * PAGE_SIZE;
            let Some(index) = page_meta::page_index(pa) else {
                if device_ok {
                    continue;
                }
                println!(
                    "[TSM] page_meta REFUSE {:?}: pa=0x{:x} untracked (page {}/{})",
                    op, pa, i, num_pages
                );
                return Err(page_meta::TransitionError::Untracked);
            };
            let current = *table
                .get(index)
                .ok_or(page_meta::TransitionError::Untracked)?;
            if let Err(e) = page_meta::next_meta(current, op, owner) {
                println!(
                    "[TSM] page_meta REFUSE {:?}: pa=0x{:x} state={:?} owner={} req={} err={:?} (page {}/{})",
                    op,
                    pa,
                    page_meta::state_of(current),
                    page_meta::owner_of(current),
                    owner,
                    e,
                    i,
                    num_pages
                );
                return Err(e);
            }
        }

        for i in 0..num_pages {
            let pa = first_pa + i * PAGE_SIZE;
            if page_meta::page_index(pa).is_none() {
                continue;
            }
            page_meta::apply(&mut table[..], pa, op, owner)?;
        }
        Ok(())
    }

    /// Mark host memory as confidential and unassigned (`ConvertPages`).
    ///
    /// The RDSM revokes the host's MPT access for this funcid once the TSM
    /// returns success, so refusing here is what keeps a page that is already
    /// confidential from being double-converted.
    ///
    /// # Safety
    /// Caller must be on the TSM's uninterruptible path.
    #[cfg(all(target_arch = "riscv64", target_os = "none"))]
    pub unsafe fn convert_pages(
        first_pa: u64,
        num_pages: u64,
    ) -> Result<(), page_meta::TransitionError> {
        Self::reserve_pages(
            first_pa,
            num_pages,
            page_meta::PageOp::Convert,
            page_meta::NO_OWNER,
        )
    }

    /// Mark host memory as TVM-shared (`ConvertSharedPages`, FID 21).
    ///
    /// The RDSM deliberately leaves the host's MPT access in place for this
    /// funcid, so the page must be tracked as shared rather than confidential;
    /// recording it as confidential would leave the state machine disagreeing
    /// with the permissions actually programmed in the MPT.
    ///
    /// # Safety
    /// Caller must be on the TSM's uninterruptible path.
    #[cfg(all(target_arch = "riscv64", target_os = "none"))]
    pub unsafe fn convert_shared_pages(
        first_pa: u64,
        num_pages: u64,
    ) -> Result<(), page_meta::TransitionError> {
        Self::reserve_pages(
            first_pa,
            num_pages,
            page_meta::PageOp::ConvertShared,
            page_meta::NO_OWNER,
        )
    }

    /// Reclaim pages back to non-confidential state (`ReclaimPages`).
    ///
    /// Refuses pages a live TVM still has mapped. Each page is zeroed before its
    /// state is flipped back to non-confidential, so once the RDSM restores the
    /// host's MPT access it can never read the previous TVM's plaintext.
    ///
    /// # Safety
    /// Caller must be on the TSM's uninterruptible path, and the range must be
    /// valid physical memory.
    #[cfg(all(target_arch = "riscv64", target_os = "none"))]
    pub unsafe fn reclaim_pages(
        first_pa: u64,
        num_pages: u64,
    ) -> Result<(), page_meta::TransitionError> {
        page_meta::validate_range(first_pa, num_pages)?;

        let mut table = page_meta::lock_table();

        // Validate the whole range first so a refusal leaves nothing changed.
        for i in 0..num_pages {
            let pa = first_pa + i * PAGE_SIZE;
            let index = page_meta::page_index(pa).ok_or(page_meta::TransitionError::Untracked)?;
            let current = *table
                .get(index)
                .ok_or(page_meta::TransitionError::Untracked)?;
            if let Err(e) =
                page_meta::next_meta(current, page_meta::PageOp::Reclaim, page_meta::NO_OWNER)
            {
                println!(
                    "[TSM] page_meta REFUSE Reclaim: pa=0x{:x} state={:?} owner={} err={:?} (page {}/{})",
                    pa,
                    page_meta::state_of(current),
                    page_meta::owner_of(current),
                    e,
                    i,
                    num_pages
                );
                return Err(e);
            }
        }

        // Commit: zero each page before flipping it back to non-confidential,
        // so the host can never read a previous TVM's plaintext once the RDSM
        // restores its MPT access.
        for i in 0..num_pages {
            let pa = first_pa + i * PAGE_SIZE;
            if page_meta::page_index(pa).is_none() {
                continue;
            }
            core::ptr::write_bytes(pa as *mut u8, 0, PAGE_SIZE as usize);
            page_meta::apply(
                &mut table[..],
                pa,
                page_meta::PageOp::Reclaim,
                page_meta::NO_OWNER,
            )?;
        }
        Ok(())
    }

    /// Initialize the G-stage page-table page pool (AddTvmPageTablePages).
    ///
    /// The donated pages must already be confidential and unassigned; on success
    /// they become TSM-private and owned by this TVM, so a later reclaim of the
    /// same range is refused while the TVM is alive.
    ///
    /// **Not an append operation.** This writes `pgt_pool_next/end` from
    /// scratch every call — a second `AddTvmPageTablePages` on the same TVM
    /// with a different range would replace the pool bounds, orphaning any
    /// intermediate PTEs already linked into live G-stage trees from the
    /// previous pool (they stay reachable through the trees but no longer
    /// count as available). The KVM host donates exactly once per TVM
    /// (`cove.c:1085`), so this is a spec-compliance gap rather than an
    /// operational bug today; expanding into an append would need
    /// `pgt_pool_next/end` to become a segment list. Until then, "donate
    /// more pages and retry" is not a supported recovery path for
    /// `PgtPoolExhausted` — the only remedy is `DestroyTvm`.
    ///
    /// # Safety
    /// The caller must guarantee `page_addr..page_addr + num_pages * PAGE_SIZE`
    /// points to valid physical memory donated for page table use.
    #[cfg(all(target_arch = "riscv64", target_os = "none"))]
    pub unsafe fn init_pgt_pool(
        &mut self,
        guest_id: usize,
        page_addr: u64,
        num_pages: u64,
    ) -> Result<(), page_meta::TransitionError> {
        Self::reserve_pages(
            page_addr,
            num_pages,
            page_meta::PageOp::AddPageTable,
            page_meta::owner_of_guest(guest_id as u8),
        )?;
        self.pgt_pool_next = page_addr;
        self.pgt_pool_end = page_addr + (num_pages * PAGE_SIZE);
        Ok(())
    }

    /// Non-riscv64 fallback: no metadata table, so record the pool bounds only.
    #[cfg(not(all(target_arch = "riscv64", target_os = "none")))]
    pub unsafe fn init_pgt_pool(&mut self, _guest_id: usize, page_addr: u64, num_pages: u64) {
        self.pgt_pool_next = page_addr;
        self.pgt_pool_end = page_addr + (num_pages * PAGE_SIZE);
    }

    /// Maximum number of mapped pages in a single block/unblock/remove batch.
    ///
    /// Bounded so that the validation pass's stack array has a compile-time size.
    #[cfg(all(target_arch = "riscv64", target_os = "none"))]
    const MAX_TRANSITION_PAGES: usize = 1024;

    /// Apply a page-state transition across all mapped pages in a GPA range.
    ///
    /// Walks the G-stage page table for `[guest_addr, guest_addr + len)`,
    /// validates each page's metadata under lock, commits all transitions
    /// atomically, then applies the appropriate PTE action (clear-valid for
    /// Block, restore-valid for Unblock, drop leaf for Remove).
    ///
    /// In this single-core POC edition the PTE action is optional for
    /// correctness (no remote TLB to worry about), but metadata MUST move.
    ///
    /// # Safety
    /// Caller must be on the TSM's uninterruptible path.
    #[cfg(all(target_arch = "riscv64", target_os = "none"))]
    unsafe fn transition_gpa_range(
        &mut self,
        guest_id: usize,
        guest_addr: u64,
        len: u64,
        op: page_meta::PageOp,
    ) -> Result<(), page_meta::TransitionError> {
        if len == 0 || len % PAGE_SIZE != 0 {
            return Err(page_meta::TransitionError::Untracked);
        }
        page_meta::validate_range(guest_addr, len / PAGE_SIZE)?;
        let owner = page_meta::owner_of_guest(guest_id as u8);
        let num_pages = len / PAGE_SIZE;

        // All-or-nothing: validate every page before committing any.
        let mut table = page_meta::lock_table();
        let mut planned = [(0u64, 0usize, 0u8); Self::MAX_TRANSITION_PAGES];
        let mut planned_len = 0usize;

        for i in 0..num_pages {
            let gpa = guest_addr + i * PAGE_SIZE;
            let Some(leaf) = super::page_table::get_pte(self.pgt_mode, self.pgt_root_addr, gpa)
            else {
                continue;
            };
            let Some(hpa) = super::page_table::pte_leaf_hpa(*leaf) else {
                continue;
            };
            let Some(index) = page_meta::page_index(hpa) else {
                continue;
            };

            let current = table[index];
            let next = page_meta::next_meta(current, op, owner)?;
            if planned_len == Self::MAX_TRANSITION_PAGES {
                return Err(page_meta::TransitionError::Untracked);
            }
            planned[planned_len] = (gpa, index, next);
            planned_len += 1;
        }

        // Commit metadata.
        for &(_, index, next) in planned.iter().take(planned_len) {
            table[index] = next;
        }
        drop(table);

        // PTE action: Block clears valid, Unblock restores it, Remove drops leaf.
        // In single-core POC this is optional but done for correctness.
        for &(gpa, _, _) in planned.iter().take(planned_len) {
            match op {
                page_meta::PageOp::Block => {
                    super::page_table::invalidate_leaf(self.pgt_mode, self.pgt_root_addr, gpa);
                }
                page_meta::PageOp::Unblock => {
                    super::page_table::revalidate_leaf(self.pgt_mode, self.pgt_root_addr, gpa);
                }
                page_meta::PageOp::Remove => {
                    super::page_table::unmap_page(self.pgt_mode, self.pgt_root_addr, gpa);
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Invalidate TVM access to a GPA range (`TvmInvalidatePages`, spec: invalidate).
    ///
    /// Marks pages as blocked so they can later be removed. In this single-core
    /// edition no remote TLB shootdown is needed.
    ///
    /// # Safety
    /// Caller must be on the TSM's uninterruptible path.
    #[cfg(all(target_arch = "riscv64", target_os = "none"))]
    pub unsafe fn block_pages(
        &mut self,
        guest_id: usize,
        guest_addr: u64,
        len: u64,
    ) -> Result<(), page_meta::TransitionError> {
        self.transition_gpa_range(guest_id, guest_addr, len, page_meta::PageOp::Block)
    }

    /// Restore a previously blocked GPA range (`TvmValidatePages`, spec: validate).
    ///
    /// # Safety
    /// Caller must be on the TSM's uninterruptible path.
    #[cfg(all(target_arch = "riscv64", target_os = "none"))]
    pub unsafe fn unblock_pages(
        &mut self,
        guest_id: usize,
        guest_addr: u64,
        len: u64,
    ) -> Result<(), page_meta::TransitionError> {
        self.transition_gpa_range(guest_id, guest_addr, len, page_meta::PageOp::Unblock)
    }

    /// Drop a blocked GPA range's mappings and release ownership
    /// (`TvmRemovePages`).
    ///
    /// This is the step that lets a later `ReclaimPages` succeed.
    ///
    /// # Safety
    /// Caller must be on the TSM's uninterruptible path.
    #[cfg(all(target_arch = "riscv64", target_os = "none"))]
    pub unsafe fn remove_pages(
        &mut self,
        guest_id: usize,
        guest_addr: u64,
        len: u64,
    ) -> Result<(), page_meta::TransitionError> {
        self.transition_gpa_range(guest_id, guest_addr, len, page_meta::PageOp::Remove)
    }

    /// Record one guest memory region (AddTvmMemoryRegion), declared by the host.
    ///
    /// Overlap with another RAM region is refused: KVM memslots do not overlap, so
    /// a declaration that does is a host bug rather than something to absorb.
    ///
    /// Overlap with a **declared MMIO window** is refused too, and for a different
    /// reason: RAM wins over MMIO in the fault classification, so a RAM region laid
    /// over a live device window would silently demote that window's accesses to
    /// demand paging and break the device. An untrusted host must not be able to do
    /// that to the guest by declaring memory.
    pub fn add_mem_region(&self, gpa: u64, size: u64) -> Result<(), TvmError> {
        let mut r = self.regions.lock();
        if r.mmio.iter().any(|m| m.overlaps(gpa, size)) {
            return Err(TvmError::BadMemoryRegion);
        }
        region_insert(&mut r.mem, gpa, size)
    }

    /// True if `gpa` falls in any declared guest memory region.
    ///
    /// This is what separates a demand-paging fault from a device access on the
    /// exit path, so it has to consider every region: judging against only the
    /// most recently declared one made the TSM disagree with the host, which
    /// classifies the same fault against all of its memslots.
    pub fn contains_gpa(&self, gpa: u64) -> bool {
        region_contains(&self.regions.lock().mem, gpa)
    }

    /// Record one MMIO region (COVG `AddMmioRegion`), declared by the guest.
    ///
    /// The guest issues this from its `ioremap` hook. Per spec the range "must be
    /// 4KB-aligned, and the region must not overlap with a previously defined
    /// region", so a repeat declaration of an already-covered window is refused
    /// rather than merged.
    ///
    /// This is stricter than `ioremap` itself, which legitimately maps several
    /// sub-page device registers landing in one page: the first such call defines
    /// the window and the rest are refused. The window stays covered either way —
    /// see [`Self::remove_mmio_region`] for the consequence on the way out.
    pub fn add_mmio_region(&self, gpa: u64, size: u64) -> Result<(), TvmError> {
        region_insert(&mut self.regions.lock().mmio, gpa, size)
    }

    /// Remove the MMIO regions overlapping a range (COVG `RemoveMmioRegion`), the
    /// guest's `iounmap`. Returns how many entries were dropped.
    ///
    /// Per spec "The TSM must remove every MMIO region that overlaps with the
    /// requested range", so this is deliberately not an exact-match delete: one
    /// request may clear several windows, and may clear a window extending past the
    /// requested range. Accesses there fault afterwards, which is the stated intent.
    ///
    /// Consequence worth knowing: because [`Self::add_mmio_region`] refuses
    /// overlap, several devices sharing one page are covered by a single entry, so
    /// the first `iounmap` of that page removes the window for all of them.
    pub fn remove_mmio_region(&self, gpa: u64, size: u64) -> Result<usize, TvmError> {
        region_remove_overlapping(&mut self.regions.lock().mmio, gpa, size)
    }

    /// True if `gpa` falls in an MMIO region the guest declared.
    ///
    /// Answering this is what authorises the fault path to hand the host a guest
    /// register, so an address the guest never declared must answer `false`: that
    /// is the difference between gating on a positive declaration and assuming
    /// anything outside RAM is a device.
    pub fn contains_mmio_gpa(&self, gpa: u64) -> bool {
        region_contains(&self.regions.lock().mmio, gpa)
    }

    /// Record a shared-memory region the guest has declared (COVG
    /// `ShareMemory`). Overlapping or abutting declarations merge — the guest
    /// shares DMA buffers piecemeal (measured boot floor: 9 declarations, 5
    /// pairwise contiguous), so re-shares and pool growth are routine, not
    /// errors. Fails only for a malformed range or a genuinely full table.
    ///
    /// Spec §10.15 requires `AddTvmSharedPages` to reject a target GPA that
    /// does not fall inside a region the TVM has declared here. This is the
    /// table that decides.
    pub fn add_shared_region(&self, gpa: u64, size: u64) -> Result<(), TvmError> {
        region_insert_coalescing(&mut self.regions.lock().shared, gpa, size)
    }

    /// Remove exactly `[gpa, gpa+size)` from the shared table (COVG
    /// `UnshareMemory`), splitting entries that extend past the range so a
    /// partial unshare does not drop still-shared neighbours. Returns how many
    /// entries were overlapped; `Err` means nothing matched, the range was
    /// malformed, or a both-sided split found no free slot (checked before any
    /// mutation — the table is never left half-trimmed).
    pub fn remove_shared_region(&self, gpa: u64, size: u64) -> Result<usize, TvmError> {
        region_remove_splitting(&mut self.regions.lock().shared, gpa, size)
    }

    /// True if `gpa` falls in a shared-memory region the guest declared.
    pub fn contains_shared_gpa(&self, gpa: u64) -> bool {
        region_contains(&self.regions.lock().shared, gpa)
    }

    /// True if all of `[gpa, gpa + size)` lies in guest-declared shared memory.
    ///
    /// Consumer of the shared-region table: `AddTvmSharedPages` calls this to
    /// decide whether the host's mapping request has an authorising guest
    /// declaration behind it. A `false` answer must translate to
    /// `SBI_ERR_INVALID_ADDRESS` at the caller so the host cannot map shared
    /// pages into GPAs the guest never opted to share.
    pub fn covers_shared_range(&self, gpa: u64, size: u64) -> bool {
        region_covers_range(&self.regions.lock().shared, gpa, size)
    }

    /// Diag: dump the shared-region table (occupancy + every live entry).
    ///
    /// 3d showed a boot-time share refused for "table full"; this measures the
    /// real peak occupancy so the capacity fix is sized from evidence, not a
    /// guess. Call only under a claimed diagnostic budget.
    pub fn dump_shared_regions(&self) {
        let regions = self.regions.lock();
        let used = regions.shared.iter().filter(|r| r.is_used()).count();
        println!(
            "[TSM] DIAG shared table: {}/{} slots used",
            used,
            regions.shared.len()
        );
        for r in regions.shared.iter().filter(|r| r.is_used()) {
            println!("[TSM] DIAG   entry gpa=0x{:x} size=0x{:x}", r.gpa, r.size);
        }
    }

    /// Claim one of this TVM's undeclared-fault diagnostic slots.
    ///
    /// Returns true for the first few faults only. The print sits on the exit path,
    /// where an unbounded `println!` is a known source of fmt-expansion panics under
    /// MTTCG, and the count is per-TVM so diagnostics are bounded.
    pub fn claim_undeclared_fault_report(&self) -> bool {
        const UNDECLARED_REPORT_LIMIT: usize = 4;
        self.undeclared_faults.fetch_add(1, Ordering::Relaxed) < UNDECLARED_REPORT_LIMIT
    }

    /// Claim one of this TVM's refused-declaration diagnostic slots.
    ///
    /// Bounded because the guest drives the declaration path and a refusal is the
    /// expected outcome for sub-page BARs sharing a page: without a cap the guest
    /// could repeat a refused declaration forever, and `println!` unwraps
    /// internally, so that would be a guest-triggerable way to panic the TSM.
    pub fn claim_mmio_refusal_report(&self) -> bool {
        const MMIO_REFUSAL_REPORT_LIMIT: usize = 4;
        self.mmio_refusals.fetch_add(1, Ordering::Relaxed) < MMIO_REFUSAL_REPORT_LIMIT
    }

    /// Claim one of this TVM's refused-shared-region diagnostic slots.
    ///
    /// Guest-side only (COVG ShareMemory/UnshareMemory). Host-side refusals get
    /// [`Self::claim_host_shared_refusal_report`]: a guest can spend a shared
    /// budget on benign refusals (a full table refuses every further
    /// declaration), and that must not silence the host trying to map shared
    /// pages into an undeclared GPA — the same mixing problem the separate MMIO
    /// counter exists to avoid, one level down.
    pub fn claim_shared_refusal_report(&self) -> bool {
        const SHARED_REFUSAL_REPORT_LIMIT: usize = 4;
        self.shared_refusals.fetch_add(1, Ordering::Relaxed) < SHARED_REFUSAL_REPORT_LIMIT
    }

    /// Claim one of this TVM's refused-`AddTvmSharedPages` diagnostic slots.
    ///
    /// Host-driven, so bounded for the same println reason; separate from the
    /// guest-side budget because this is the one that reports an actual
    /// authorisation failure rather than a routine declaration refusal.
    pub fn claim_host_shared_refusal_report(&self) -> bool {
        const HOST_SHARED_REFUSAL_REPORT_LIMIT: usize = 4;
        self.host_shared_refusals.fetch_add(1, Ordering::Relaxed) < HOST_SHARED_REFUSAL_REPORT_LIMIT
    }

    /// Claim one of this TVM's `pgt_pool exhausted` diagnostic slots.
    ///
    /// Bounded because `ConvertAiaImsic` loops `MAX_VCPUS` times per hart
    /// and every failed allocation prints one line, so an
    /// undersized pool plus repeated FID calls could spam the console.
    pub fn claim_pgt_pool_report(&self) -> bool {
        const PGT_POOL_REPORT_LIMIT: usize = 4;
        self.pgt_pool_reports.fetch_add(1, Ordering::Relaxed) < PGT_POOL_REPORT_LIMIT
    }

    /// Whether this TVM's state permits `AddTvmZeroPages`.
    ///
    /// spec: "This call may be made only after calling `sbi_covh_finalize_tvm()`"
    /// — zero pages are the demand-fault path, so accepting them earlier would let
    /// a host grow the TVM while its measurement is still open.
    ///
    /// The decision lives here rather than at the call site because the COVH
    /// dispatch module is compiled only for the target, so a decision made there
    /// cannot be unit tested at all. This function is the decision; the call site
    /// is the wiring, and the wiring remains untested.
    pub fn may_add_zero_pages(&self) -> bool {
        self.state == Some(TvmState::Runnable)
    }

    /// Whether this TVM's state permits COVI `SetImsicAddr`.
    ///
    /// spec: "This can be called only after `sbi_covi_init_tvm_aia()` and
    /// **before** `sbi_covh_finalize_tvm()`", and that function's error table names
    /// the state directly. The "after AiaInit" half is not enforceable while
    /// AiaInit is a no-op.
    pub fn may_set_imsic_addr(&self) -> bool {
        self.state == Some(TvmState::Initializing)
    }

    /// Whether this TVM's state permits `CreateTvmVcpu`.
    ///
    /// spec Table 27 `:4553-4555`: "vCPUs may not be added after the TVM is
    /// finalized" — the measurement covers the vCPU list, so accepting a new
    /// vCPU on a finalized TVM would grow measured content behind the attestor.
    pub fn may_create_vcpu(&self) -> bool {
        self.state == Some(TvmState::Initializing)
    }

    /// Whether this TVM's state and this vCPU's history allow `RunTvmVcpu`.
    ///
    /// Two conditions, both spec `sbi_covh_run_tvm_vcpu` §10.17:
    /// - The TVM must be in the runnable state.
    /// - A vCPU that returned a non-zero sbiret.value on a previous run is
    ///   quarantined for the rest of the TVM's lifetime; subsequent runs
    ///   with the same vcpu_id must fail.
    ///
    /// The function includes its own bounds check on `vcpu_idx` as a safety
    /// belt: the sole production caller (`handle_tvm_cpu_run`) has already
    /// rejected out-of-range ids with `INVALID_PARAM`, but keeping the
    /// check here guards against any future caller indexing past
    /// `self.vcpus`.
    pub fn may_run_vcpu(&self, vcpu_idx: usize) -> bool {
        if self.state != Some(TvmState::Runnable) {
            return false;
        }
        if vcpu_idx >= MAX_VCPUS {
            return false;
        }
        !self.vcpus[vcpu_idx].is_terminated()
    }

    /// Whether `gpa` is already claimed as the IMSIC address of a *different* vCPU.
    ///
    /// spec: "No two vCPUs may share the same `tvm_vcpu_imsic_gpa`" — sharing one
    /// interrupt file between vCPUs would cross their interrupts. `vcpu_idx` is
    /// excluded on purpose: the host may re-issue the same address for the same
    /// vCPU, and comparing a slot against itself would reject that.
    pub fn imsic_gpa_claimed_by_other(&self, vcpu_idx: usize, gpa: u64) -> bool {
        (0..MAX_VCPUS).any(|i| {
            i != vcpu_idx && self.vcpus[i].imsic_gpa != 0 && self.vcpus[i].imsic_gpa == gpa
        })
    }

    /// Check that `state_page_addr` and `num_pages` describe a valid donation
    /// for `CreateTvmVcpu`.
    ///
    /// spec `:4539-4542`: the address must be page-aligned, must point to
    /// confidential memory, and the range must be `tsm_info.tvm_vcpu_state_pages`
    /// pages long. A legal host allocates via `alloc_pages` (physical base is
    /// naturally page-aligned), converts the range with `ConvertPages` (each
    /// frame becomes `ConfUnassigned`), and sizes by the value the TSM reports —
    /// so this check is a no-op on the happy path and closes the fail-open door
    /// for a malicious host handing in an arbitrary address.
    ///
    /// A confidential-unassigned frame is the state a fresh donation lands in;
    /// anything else (host-owned, already assigned to a TVM, TSM-private, ...)
    /// means the host did not donate this range for this call.
    #[cfg(all(target_arch = "riscv64", target_os = "none"))]
    pub fn check_vcpu_state_pages(state_page_addr: u64, num_pages: u64) -> bool {
        if state_page_addr % PAGE_SIZE != 0 {
            return false;
        }
        // Bounds + wrap-around before indexing the metadata table.
        if page_meta::validate_range(state_page_addr, num_pages).is_err() {
            return false;
        }
        let table = page_meta::lock_table();
        for i in 0..num_pages {
            let pa = state_page_addr + i * PAGE_SIZE;
            let Some(index) = page_meta::page_index(pa) else {
                return false;
            };
            if page_meta::state_of(table[index]) != page_meta::PageState::ConfUnassigned {
                return false;
            }
        }
        true
    }

    /// Non-riscv64 fallback: no metadata table, so only the shape of the range
    /// can be checked (alignment, non-zero length, no wrap). The metadata check
    /// runs on target.
    #[cfg(not(all(target_arch = "riscv64", target_os = "none")))]
    pub fn check_vcpu_state_pages(state_page_addr: u64, num_pages: u64) -> bool {
        if state_page_addr % PAGE_SIZE != 0 {
            return false;
        }
        if num_pages == 0 {
            return false;
        }
        state_page_addr.checked_add(num_pages * PAGE_SIZE).is_some()
    }

    /// Check that `[dest_addr, dest_addr + len)` is a byte range the TSM may
    /// safely write to on the host's behalf: every page it touches must be
    /// tracked RAM (so a page-state entry exists) and be in state
    /// `NonConfidential` (so the TSM is not about to trample a page some TVM
    /// still owns).
    ///
    /// Called from `GetTsmInfo` where the host asks the TSM to copy `TsmInfo`
    /// into a buffer the host owns. Spec §10.2 requires `SBI_ERR_INVALID_ADDRESS`
    /// if the destination is not accessible for that write; skipping the check
    /// let a malicious host aim `dest_addr` at a page the RDSM would ultimately
    /// bounce, but only after the TSM had already opened its write, so this
    /// pushes the refusal up into the SBI response the host can act on.
    ///
    /// `len == 0` is refused: a zero-length write is meaningless and would let
    /// a host probe the check with a range the wrap-around math treats as
    /// empty.
    #[cfg(all(target_arch = "riscv64", target_os = "none"))]
    pub fn check_host_writable_range(dest_addr: u64, len: u64) -> bool {
        if len == 0 {
            return false;
        }
        let Some(last_byte) = dest_addr.checked_add(len - 1) else {
            return false;
        };
        let first_page = dest_addr & !(PAGE_SIZE - 1);
        let last_page = last_byte & !(PAGE_SIZE - 1);
        let table = page_meta::lock_table();
        let mut page = first_page;
        loop {
            let Some(index) = page_meta::page_index(page) else {
                return false;
            };
            if page_meta::state_of(table[index]) != page_meta::PageState::NonConfidential {
                return false;
            }
            if page == last_page {
                return true;
            }
            page += PAGE_SIZE;
        }
    }

    /// Non-riscv64 fallback: no metadata table, so only the shape of the range
    /// can be checked (non-zero length, no wrap). The confidentiality check runs
    /// on target.
    #[cfg(not(all(target_arch = "riscv64", target_os = "none")))]
    pub fn check_host_writable_range(dest_addr: u64, len: u64) -> bool {
        if len == 0 {
            return false;
        }
        dest_addr.checked_add(len - 1).is_some()
    }

    /// Map `num_pages` zero pages at `guest_addr` (AddTvmZeroPages),
    /// then flush with sfence.vma + hfence.gvma.
    ///
    /// Each page is scrubbed before it is mapped. The spec requires the TSM to
    /// zero a confidential page before handing it to a TVM, and without it a
    /// frame released by a previous TVM but never reclaimed by the host would
    /// reach the next TVM still carrying the old guest's plaintext.
    ///
    /// # Safety
    /// The caller must guarantee `page_addr..page_addr + num_pages * PAGE_SIZE`
    /// points to valid physical memory and that this TVM's page-table pool has
    /// been initialized via `init_pgt_pool`.
    #[cfg(all(target_arch = "riscv64", target_os = "none"))]
    pub unsafe fn add_zero_pages(
        &mut self,
        guest_id: usize,
        page_addr: u64,
        num_pages: u64,
        guest_addr: u64,
    ) -> Result<(), page_meta::TransitionError> {
        self.reject_existing_mappings(guest_addr, num_pages)?;
        Self::reserve_pages(
            page_addr,
            num_pages,
            page_meta::PageOp::AddZero,
            page_meta::owner_of_guest(guest_id as u8),
        )?;
        for i in 0..num_pages {
            let gpa = guest_addr + i * PAGE_SIZE;
            let hpa = page_addr + i * PAGE_SIZE;
            ptr::write_bytes(hpa as *mut u8, 0, PAGE_SIZE as usize);
            if !crate::page_table::map_page(self.pgt_mode, self.pgt_root_addr, gpa, hpa) {
                return Err(page_meta::TransitionError::PgtPoolExhausted);
            }
        }
        core::arch::asm!("sfence.vma zero, zero");
        riscv_regs::hfence_gvma!();
        Ok(())
    }

    /// Copy `num_pages` pages from `src_addr` to `dest_addr` and map them
    /// at `guest_addr` (AddTvmMeasuredPages).
    ///
    /// Copies and maps pages into the TVM's address space. No cryptographic
    /// measurement is computed in this edition. No fence: matches the original
    /// handler, which relied on the subsequent FinalizeTvm/first-run path for TLB
    /// visibility.
    ///
    /// No separate scrub is needed here: the full-page copy overwrites every
    /// byte of the destination frame.
    ///
    /// # Safety
    /// The caller must guarantee `src_addr`/`dest_addr` each cover
    /// `num_pages` pages of valid physical memory (non-overlapping) and
    /// that this TVM's page-table pool has been initialized.
    #[cfg(all(target_arch = "riscv64", target_os = "none"))]
    pub unsafe fn add_measured_pages(
        &mut self,
        guest_id: usize,
        src_addr: u64,
        dest_addr: u64,
        num_pages: u64,
        guest_addr: u64,
    ) -> Result<(), page_meta::TransitionError> {
        self.reject_existing_mappings(guest_addr, num_pages)?;
        Self::reserve_pages(
            dest_addr,
            num_pages,
            page_meta::PageOp::AddMeasured,
            page_meta::owner_of_guest(guest_id as u8),
        )?;
        for i in 0..num_pages {
            let src = (src_addr + i * PAGE_SIZE) as *const u8;
            let dst = (dest_addr + i * PAGE_SIZE) as *mut u8;
            let gpa = guest_addr + i * PAGE_SIZE;
            ptr::copy_nonoverlapping(src, dst, PAGE_SIZE as usize);
            if !crate::page_table::map_page(
                self.pgt_mode,
                self.pgt_root_addr,
                gpa,
                dest_addr + i * PAGE_SIZE,
            ) {
                return Err(page_meta::TransitionError::PgtPoolExhausted);
            }
        }
        Ok(())
    }

    /// Map `num_pages` shared pages at `guest_addr` (AddTvmSharedPages),
    /// then flush with hfence.gvma only (no sfence.vma — matches the
    /// original handler).
    ///
    /// Only host-owned or already-shared-to-this-TVM frames are accepted, so a
    /// confidential page cannot be smuggled in through the shared path and one
    /// TVM cannot pick up a frame another TVM shares.
    ///
    /// # Safety
    /// The caller must guarantee `page_addr..page_addr + num_pages * PAGE_SIZE`
    /// points to valid non-confidential physical memory and that this TVM's
    /// page-table pool has been initialized.
    #[cfg(all(target_arch = "riscv64", target_os = "none"))]
    pub unsafe fn add_shared_pages(
        &mut self,
        guest_id: usize,
        page_addr: u64,
        num_pages: u64,
        guest_addr: u64,
    ) -> Result<(), page_meta::TransitionError> {
        self.reject_existing_mappings(guest_addr, num_pages)?;
        Self::reserve_pages(
            page_addr,
            num_pages,
            page_meta::PageOp::AddShared,
            page_meta::owner_of_guest(guest_id as u8),
        )?;
        for i in 0..num_pages {
            let gpa = guest_addr + i * PAGE_SIZE;
            let hpa = page_addr + i * PAGE_SIZE;
            if !crate::page_table::map_page(self.pgt_mode, self.pgt_root_addr, gpa, hpa) {
                return Err(page_meta::TransitionError::PgtPoolExhausted);
            }
        }
        riscv_regs::hfence_gvma!();
        Ok(())
    }

    /// Initialize a vCPU slot for this TVM (CreateTvmVcpu): record the
    /// vcpu id, reset the physical-hart binding to the unbound sentinel
    /// (`MAX_HARTS`) and mark the slot Created.
    ///
    /// # Safety
    /// Caller must ensure `vcpu_id < MAX_VCPUS` and have exclusive access
    /// to this TVM's state (`&mut self`).
    pub unsafe fn create_vcpu(&mut self, vcpu_id: usize) {
        self.vcpus[vcpu_id].create(vcpu_id);
    }

    /// Finalize the TVM (FinalizeTvm): record the entry point, transition the
    /// lifecycle state to `Runnable`, power on the boot vCPU and park the
    /// remaining created vCPUs as PoweredOff.
    ///
    /// # Safety
    /// The caller must have verified the TVM is in the `Initializing`
    /// state and have exclusive access to it (`&mut self`); no vCPU of
    /// this TVM may be running concurrently.
    pub unsafe fn finalize(&mut self, entry_sepc: u64, entry_arg: u64) {
        self.entry_sepc = entry_sepc;
        self.entry_arg = entry_arg;
        self.state = Some(TvmState::Runnable);

        for (i, vcpu) in self.vcpus.iter_mut().enumerate() {
            if vcpu.get_status() == VcpuStatus::Created {
                if i == 0 {
                    let _ = vcpu.power_on(entry_sepc, entry_arg);
                } else {
                    vcpu.set_status(VcpuStatus::PoweredOff);
                }
            }
        }
    }
}

/// Zero-initialized static TVM state.
// SAFETY: Tvm is a plain-old-data struct with no invalid bit patterns; zeroing
// all bytes produces a valid initial state (all integers zero, all Options None).
static mut TVM: Tvm = unsafe { mem::MaybeUninit::zeroed().assume_init() };

/// Per-hart mapping: hart_id → vcpu_id currently running on that hart.
static mut CURRENT_VCPU_ID: [usize; MAX_HARTS] = [0; MAX_HARTS];

/// Per-hart mapping: hart_id → guest_id currently running on that hart.
static mut CURRENT_GUEST_ID: [usize; MAX_HARTS] = [0; MAX_HARTS];

/// Get a raw pointer to the TVM state by guest_id.
///
/// Returns `*mut Tvm` rather than `&'static mut Tvm` on purpose: several call
/// chains already hold a `&mut Tvm` when they reach here, and handing out a
/// second `&'static mut` to the same static would break `&mut` exclusivity
/// (undefined behaviour even on a single hart). Callers materialize a single
/// short-lived `&mut *ptr` when they need one.
///
/// # Safety
/// Reads the global `TVM` state; the caller must be on the TSM's
/// single-threaded ecall path and must not create two overlapping `&mut Tvm`
/// from the returned pointer.
#[inline(always)]
pub unsafe fn get_tvm_by_id(guest_id: usize) -> *mut Tvm {
    assert!(
        guest_id == 0,
        "guest_id {} out of range (single-TVM POC)",
        guest_id
    );
    core::ptr::addr_of_mut!(TVM)
}

/// Generation counter, incremented on each DestroyTvm.
///
/// A handle minted at create time embeds the generation, so any handle
/// issued before the destroy fails the generation comparison in
/// [`verify_tvm_handle`] — the host cannot address the new occupant with a
/// handle for its previous one.
static GENERATION_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Encode a generation into an opaque 64-bit TVM handle.
///
/// This single-TVM POC has one slot (index 0), so the handle carries only
/// the generation.
#[inline]
pub fn encode_tvm_handle(generation: u64) -> u64 {
    generation
}

/// Decode an opaque TVM handle into its (slot, generation) pair.
///
/// The slot is always 0 in this single-TVM POC.
#[inline]
pub fn decode_tvm_handle(handle: u64) -> (usize, u64) {
    (0, handle)
}

/// The error [`verify_tvm_handle`] answers for any handle it cannot honour.
///
/// Deliberately carries no detail: every COVH/COVI entry point maps it to
/// `SBI_ERR_INVALID_PARAM`, and distinguishing "never existed" from "stale"
/// would tell a buggy or malicious host more than the spec requires.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidTvmHandle;

/// Validate a host-supplied TVM handle.
///
/// Decodes the generation, refuses a free slot, and refuses a generation
/// that does not match the current one (a stale handle for a destroyed
/// TVM). Returns the slot index (always 0) on success so every COVH/COVI
/// entry point resolves the handle exactly once.
///
/// # Safety
/// Reads the global `TVM` state; the caller must be on the TSM's
/// single-threaded ecall path (same contract as [`get_tvm_by_id`]).
#[inline]
pub unsafe fn verify_tvm_handle(handle: u64) -> Result<usize, InvalidTvmHandle> {
    let (slot, generation) = decode_tvm_handle(handle);
    if (*get_tvm_by_id(slot)).state.is_none() {
        return Err(InvalidTvmHandle);
    }
    if generation != GENERATION_COUNTER.load(Ordering::Acquire) {
        return Err(InvalidTvmHandle);
    }
    Ok(slot)
}

/// Return the single TVM slot (index 0) if it is free, or `None` otherwise.
///
/// The spec requires a destroyed TVM's resources to become assignable
/// again, so the slot is reused after teardown.
///
/// # Safety
/// Dereferences the raw pointer returned by [`get_tvm_by_id`]; same
/// contract applies.
pub unsafe fn find_free_tvm_slot() -> Option<usize> {
    if (*get_tvm_by_id(0)).state.is_none() {
        Some(0)
    } else {
        None
    }
}

/// Get a raw pointer to the current hart's TVM state.
///
/// See [`get_tvm_by_id`] for why this returns `*mut Tvm` rather than a
/// `&'static mut Tvm`.
///
/// # Safety
/// Same contract as [`get_tvm_by_id`].
#[inline(always)]
pub unsafe fn get_current_tvm() -> *mut Tvm {
    get_tvm_by_id(current_guest_id())
}

/// Store the current hart_id into the tp register for later retrieval.
/// Must be called once per hart during initialization (before any vCPU runs).
/// RDSM saves/restores tp per-hart across domain switches, so this is safe.
///
/// # Safety
/// Clobbers the `tp` register. Must only be called during single-threaded
/// per-hart init before any vCPU enters the guest.
#[cfg(target_arch = "riscv64")]
#[inline(always)]
pub unsafe fn store_hart_id(hart_id: u64) {
    core::arch::asm!("mv tp, {}", in(reg) hart_id);
}

#[cfg(not(target_arch = "riscv64"))]
#[inline(always)]
pub unsafe fn store_hart_id(_hart_id: u64) {}

/// Get the current hart_id from the tp register.
///
/// # Safety
/// [`store_hart_id`] must have been called on this hart during init;
/// reading `tp` before that yields an undefined value.
#[cfg(target_arch = "riscv64")]
#[inline(always)]
pub unsafe fn get_hart_id() -> usize {
    let hart_id: usize;
    core::arch::asm!("mv {}, tp", out(reg) hart_id);
    hart_id
}

#[cfg(not(target_arch = "riscv64"))]
#[inline(always)]
pub unsafe fn get_hart_id() -> usize {
    0
}

/// Set the current vcpu_id for this hart.
///
/// # Safety
/// Indexes `CURRENT_VCPU_ID` by [`get_hart_id`]; caller must ensure
/// `store_hart_id` was called during init.
#[inline(always)]
pub unsafe fn set_current_vcpu_id(vcpu_id: usize) {
    CURRENT_VCPU_ID[get_hart_id()] = vcpu_id;
}

/// Get the current vcpu_id for this hart.
///
/// # Safety
/// Same precondition as [`set_current_vcpu_id`].
#[inline(always)]
pub unsafe fn current_vcpu_id() -> usize {
    CURRENT_VCPU_ID[get_hart_id()]
}

/// Set the current guest_id for this hart.
///
/// # Safety
/// Indexes `CURRENT_GUEST_ID` by [`get_hart_id`]; caller must ensure
/// `store_hart_id` was called during init.
#[inline(always)]
pub unsafe fn set_current_guest_id(guest_id: usize) {
    CURRENT_GUEST_ID[get_hart_id()] = guest_id;
}

/// Get the current guest_id for this hart.
///
/// # Safety
/// Same precondition as [`set_current_guest_id`].
#[inline(always)]
pub unsafe fn current_guest_id() -> usize {
    CURRENT_GUEST_ID[get_hart_id()]
}
