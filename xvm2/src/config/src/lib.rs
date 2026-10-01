// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! Platform configuration constants for the CoVE TSM.
//!
//! This crate is the single point of definition for all tunable platform
//! limits and layout constants. Any change to a value below must be made
//! here and nowhere else; other crates import these constants instead of
//! defining their own copies.
//!
//! # External couplings (manual synchronization required)
//!
//! The following places outside this crate encode the same values and must
//! be updated by hand whenever the corresponding constant changes:
//!
//! 1. `tsm.lds` stack layout: the boot-hart stack reserves `0x80000` bytes,
//!    which corresponds to `MAX_HARTS = 1` (a single hart stack of
//!    `HYP_STACK_SIZE` bytes).

#![no_std]

/// Maximum number of vCPUs supported per TVM.
pub const MAX_VCPUS: usize = 1;

/// Number of confidential pages the host donates per vCPU on `CreateTvmVcpu`.
///
/// Reported to the host in `TsmInfo.tvm_vcpu_state_pages`, so the host allocates
/// this many pages, runs them through `ConvertPages`, then hands the base
/// address to `CreateTvmVcpu` in `state_page_addr`. This constant therefore drives
/// both what the TSM reports and what it accepts — one source of truth so those
/// two cannot drift.
///
/// **The 2 pages are validated but not consumed.** `Tvm::check_vcpu_state_pages`
/// (target-side) verifies each donated frame is `ConfUnassigned` and reserves it,
/// but `state_page_addr` is not stored in any TSM data structure and is neither
/// read nor written afterwards. Per-vCPU state actually lives in the compile-
/// time `static mut TVM: Tvm` instance baked into the TSM binary — a
/// deliberate simplification that keeps the TSM stateless-looking but violates
/// the intent of `CAP_MEMORY_ALLOCATION`, which is that the TSM has no memory of
/// its own and per-vCPU control structures live in host-donated pages (as in
/// other reference implementations like TDX TDVPR/TDVPS or ARM CCA REC).
///
/// The value `2` is therefore arbitrary. It is neither aligned with the per-vCPU
/// shared-memory geometry (`SHMEM_PAGES_PER_VCPU = 3`, host-shared, held elsewhere in the
/// `tvm_state_addr` region) nor sized to actually hold a `Vcpu` (which currently
/// weighs ~several KB but is not stored in these pages anyway). Reporting `0`
/// would collide with the TSM having advertised `COVE_TSM_CAP_MEMORY_ALLOCATION`,
/// so `2` is retained as a spec-conformant placeholder. The tracked gap and its
/// architectural fix (migrate `Vcpu` storage into `state_page_addr`) remain known
/// limitations of this POC.
pub const TVM_VCPU_STATE_PAGES: u64 = 2;

/// Maximum number of guest memory regions a TVM may declare.
///
/// The host calls `AddTvmMemoryRegion` once per KVM memslot, so this bounds how
/// many memslots a TVM can have. Eight covers the handful a typical guest uses
/// (main RAM plus any split low/high windows) while keeping the table small
/// enough to sit in the zero-initialized `TVM` instance.
pub const MAX_MEM_REGIONS: usize = 8;

/// Maximum number of MMIO regions a TVM's guest may declare.
///
/// Sized from a measured boot rather than a guess. On this project's `lkvm`
/// configuration (virtio-pci, 8250 console, AIA/IMSIC) a Linux guest settles at
/// **8 distinct regions**:
///
/// | region | size | device |
/// |---|---|---|
/// | `0x0800_0000` | 0x2000 | IMSIC |
/// | `0x0000_0000` | 0x10000 | PCI I/O space |
/// | `0x3000_0000` | 0x200000 | PCI ECAM |
/// | `0x4000_0000` | 0x1000 | PCI BARs (three sub-page BARs share this page) |
/// | `0x1000_0000`–`0x1000_3000` | 0x1000 each | 8250, four ports |
///
/// So 16 leaves 100% headroom over the observed peak. Note the bound counts
/// *distinct* regions, not `ioremap` calls: the spec forbids overlapping
/// declarations, so the repeat declarations that sub-page BARs generate are
/// refused and consume no slot. The same boot issued **22** declarations to reach
/// those 8 regions — 14 of them refused, all for the shared `0x4000_0000` page.
///
/// Declarations past this bound are refused rather than dropped, and refusal is
/// fail-closed — the fault path will treat that window as ordinary memory, so the
/// device stops working rather than leaking a guest register. Raise this before
/// adding device classes that map many separate windows.
pub const MAX_MMIO_REGIONS: usize = 16;

/// Maximum number of shared-memory regions a TVM may have declared at once.
///
/// The guest declares a region by calling `sbi_covg_share_memory_region` (COVG
/// FID #2). Each declaration is recorded in the per-TVM table so that when the
/// host later calls `AddTvmSharedPages` (COVH FID #13), the TSM can verify the
/// target range falls inside memory the guest actually asked to share.
///
/// Unlike the MMIO table, entries here merge: `set_memory_decrypted` is called
/// once per DMA buffer by swiotlb and every virtio driver, so contiguous and
/// repeated declarations are routine. Overflow answers the guest an error,
/// which `set_memory_decrypted` propagates and DMA setup fails — raise this
/// before adding device classes that share many non-contiguous windows.
pub const MAX_SHARED_REGIONS: usize = 16;

/// Maximum number of physical harts supported.
///
/// Sizes per-hart arrays. Each vCPU is bound to one hart, so `MAX_VCPUS`
/// is expected to stay within `MAX_HARTS`; this relationship is not
/// enforced at compile time.
pub const MAX_HARTS: usize = 1;

/// 4KB base page size.
pub const PAGE_SIZE: u64 = 4096;

/// Base physical address of RAM tracked by the page state machine.
///
/// Mirrors `RAM_START` in the M-mode RDSM's MPT enforcement, which is where
/// RAM management at 4KB granularity begins.
pub const RAM_START: u64 = 0x8000_0000;

/// Size of the physical range covered by the page state machine.
///
/// Deliberately 4GB rather than the 2GB the RDSM's MPT comment claims,
/// because the QEMU command line in `build.sh` boots with `-m 4G`.
/// Sizing this short would leave the upper RAM pages untracked, and an untracked
/// page reads back as `NonConfidential`, which would silently defeat the state
/// checks for any page the host allocates above 2GB.
pub const TRACKED_RAM_SIZE: u64 = 4 * 1024 * 1024 * 1024;

/// Number of 4KB pages tracked by the page state machine.
///
/// One metadata byte per page, so this is also the size of the metadata array
/// in bytes: 1MB, which fits comfortably in the 128MB `tsm.lds` link window.
pub const TRACKED_PAGES: usize = (TRACKED_RAM_SIZE / PAGE_SIZE) as usize;

/// Address of the hypervisor stack top.
pub const HYP_STACK_TOP: u64 = 0xffff_ffff_ffe0_0000;

/// Number of 4k-pages reserved for each CPU as their stack.
pub const HYP_STACK_PAGES: u64 = 0x80;

/// Size of stack in bytes.
pub const HYP_STACK_SIZE: u64 = HYP_STACK_PAGES * PAGE_SIZE;

/// End of stack (lower address)
pub const HYP_STACK_BOTTOM: u64 = HYP_STACK_TOP - HYP_STACK_SIZE;

// Compile-time sanity checks.
const _: () = assert!(MAX_VCPUS > 0 && MAX_HARTS > 0);
// Verify stack top is 2M-aligned.
const _: () = assert!(HYP_STACK_TOP % (2 * 1024 * 1024) == 0);
const _: () = assert!(HYP_STACK_SIZE == HYP_STACK_PAGES * PAGE_SIZE);
const _: () = assert!(RAM_START % PAGE_SIZE == 0);
const _: () = assert!(TRACKED_RAM_SIZE % PAGE_SIZE == 0);
const _: () = assert!(TRACKED_PAGES as u64 * PAGE_SIZE == TRACKED_RAM_SIZE);
