// SPDX-FileCopyrightText: 2026 DAMO Inc.
// SPDX-FileCopyrightText: 2023 Rivos Inc.
//
// SPDX-License-Identifier: Apache-2.0

use crate::error::*;
use crate::function::*;

/// Layout of `scratch` in the `Shmem` structure when used with `RunTvmVcpu`. Used to communicate
/// a TVM's exit status to the host.
#[repr(C)]
pub struct TsmShmemScratch {
    /// General purpose registers for a TVM guest.
    ///
    /// The TSM will always read or write the minimum number of registers in this set to complete
    /// the requested action, in order to avoid leaking information from the TVM.
    ///
    /// The TSM will write to these registers upon return from `RunTvmVcpu` when:
    ///  - The vCPU takes a store guest page fault in an emulated MMIO region.
    ///  - The vCPU makes an ECALL that is to be forwarded to the host.
    ///
    /// The TSM will read from these registers when:
    ///  - The vCPU takes a load guest page fault in an emulated MMIO region.
    pub guest_gprs: [u64; 32],
    _reserved: [u64; 224],
}

impl Default for TsmShmemScratch {
    fn default() -> Self {
        Self {
            guest_gprs: [0; 32],
            _reserved: [0; 224],
        }
    }
}

/// Provides the state of the confidential VM supervisor.
#[repr(u32)]
#[derive(Copy, Clone, PartialEq, Eq, Default)]
pub enum TsmState {
    /// TSM has not been loaded on this platform.
    #[default]
    TsmNotLoaded = 0,
    /// TSM has been loaded, but has not yet been initialized.
    TsmLoaded = 1,
    /// TSM has been loaded & initialized, and is ready to accept TEECALLs.
    TsmReady = 2,
}

/// CoVE TSM capability bit positions (per spec v0.7 §10.2 `tsm_capabilities`).
/// The values are shared verbatim with the Linux/KVM consumer side.
/// Capability: TSM can promote a host VM into a confidential TVM.
pub const COVE_TSM_CAP_PROMOTE_TVM: u32 = 0;
/// Capability: local attestation is supported.
pub const COVE_TSM_CAP_ATTESTATION_LOCAL: u32 = 1;
/// Capability: remote attestation is supported.
pub const COVE_TSM_CAP_ATTESTATION_REMOTE: u32 = 2;
/// Capability: AIA (Advanced Interrupt Architecture) virtualization is supported.
pub const COVE_TSM_CAP_AIA: u32 = 3;
/// Capability: MRIF (Memory-Resident Interrupt File) is supported.
pub const COVE_TSM_CAP_MRIF: u32 = 4;
/// Capability: dynamic memory allocation for TVMs is supported.
pub const COVE_TSM_CAP_MEMORY_ALLOCATION: u32 = 5;

/// CoVE TSM implementation identifiers (spec v0.7 Table 10).
pub const COVE_TSM_IMPL_ID_1: u32 = 1;
/// ACE reference TSM (spec-assigned).
pub const COVE_TSM_IMPL_ID_2: u32 = 2;

/// Placeholder value reported by this implementation while the CoVE working
/// group has not assigned an official ID.
///
/// Not defined by Table 10 — the table lists `1`, `2`, and `>2 RESERVED`, so
/// `0` is a table-outside value. It is picked over any `>2` value because
/// the reserved range is what a future assignment will draw from, so any
/// self-chosen value there is guaranteed to collide sooner or later;
/// `0` sits outside that range and cannot be assigned to another
/// implementation without an explicit table change.
/// NOTE: this placeholder stands in until the CoVE working group assigns an
/// official implementation ID.
pub const COVE_TSM_IMPL_UNASSIGNED: u32 = 0;

/// Major version number of this TSM implementation.
pub const TSM_VERSION_MAJOR: u32 = 2;
/// Minor version number of this TSM implementation.
pub const TSM_VERSION_MINOR: u32 = 0;

/// Information returned from the system about the entity that manages confidential VMs and
/// confidential memory isolation.
///
/// Binary layout (RV64/LP64, spec v0.7, total 48 bytes):
/// `@0 tsm_state`, `@4 tsm_impl_id`, `@8 tsm_version`, `@12 padding`,
/// `@16 tsm_capabilities`, `@24 tvm_state_pages`, `@32 tvm_max_vcpus`,
/// `@40 tvm_vcpu_state_pages`.
#[repr(C)]
#[derive(Default)]
pub struct TsmInfo {
    /// The current state of the TSM. If the state is not `TsmReady`, the remaining fields are
    /// invalid and will be initialized to 0. (`@0`)
    pub tsm_state: TsmState,
    /// Identifier of the TSM implementation, one of the `COVE_TSM_IMPL_*`
    /// constants above (spec Table 10 assigns IDs 1 and 2 to reference
    /// implementations; `>2` is reserved). This implementation reports
    /// `COVE_TSM_IMPL_UNASSIGNED` pending an official assignment. (`@4`)
    pub tsm_impl_id: u32,
    /// Version number of the running TSM, encoded as `(MAJOR << 16) | MINOR`. (`@8`)
    pub tsm_version: u32,
    /// Bitmap of optional TSM capabilities, built from the `COVE_TSM_CAP_*` bit
    /// positions. (`@16`, preceded by 4 bytes of compiler-inserted padding at `@12`)
    pub tsm_capabilities: u64,
    /// The number of 4kB pages which must be donated to the TSM for storing TVM state in the
    /// `CreateTvm` TEECALL. (`@24`)
    pub tvm_state_pages: u64,
    /// The maximum number of vCPUs a TVM can support. (`@32`)
    pub tvm_max_vcpus: u64,
    /// The number of 4kB pages which must be donated to the TSM when creating a new VCPU. (`@40`)
    pub tvm_vcpu_state_pages: u64,
}

// Compile-time layout assertion: the spec v0.7 `TsmInfo` structure must be
// exactly 48 bytes on RV64/LP64 so the binary layout matches the Linux/KVM
// consumer side byte-for-byte.
const _: () = assert!(core::mem::size_of::<TsmInfo>() == 48);

/// Parameters used for creating a new confidential VM.
#[repr(C)]
pub struct TvmCreateParams {
    /// The base physical address of the 16kB confidential memory region that should be used for the
    /// TVM's page directory. Must be 16kB-aligned.
    pub tvm_page_directory_addr: u64,
    /// The base physical address of the confidential memory region to be used to hold the TVM's
    /// global state. Must be page-aligned and `TsmInfo::tvm_state_pages` pages in length.
    pub tvm_state_addr: u64,
}

/// Types of pages allowed to be used for creating or managing confidential VMs.
#[repr(u64)]
#[derive(Copy, Clone, PartialEq, Eq, Default, Debug)]
pub enum TsmPageType {
    #[default]
    /// Standard 4k pages.
    Page4k = 0,
    /// 2 Megabyte pages.
    Page2M = 1,
    /// 1 Gigabyte pages.
    Page1G = 2,
    /// 512 Gigabyte pages.
    Page512G = 3,
}

impl TsmPageType {
    /// Attempts to create a page type from the given u64 register value. Returns an error if the
    /// value is greater than 3(512GB).
    pub fn from_reg(reg: u64) -> Result<Self> {
        use TsmPageType::*;
        match reg {
            0 => Ok(Page4k),
            1 => Ok(Page2M),
            2 => Ok(Page1G),
            3 => Ok(Page512G),
            _ => Err(Error::InvalidParam),
        }
    }

    /// Returns the size of this page type in bytes.
    pub fn size_bytes(&self) -> u64 {
        match self {
            TsmPageType::Page4k => 4096,
            TsmPageType::Page2M => 2 * 1024 * 1024,
            TsmPageType::Page1G => 1024 * 1024 * 1024,
            TsmPageType::Page512G => 512 * 1024 * 1024 * 1024,
        }
    }
}

/// Functions provided by the COVE Host extension.
#[derive(Copy, Clone, Debug)]
pub enum CoveHostFunction {
    /// Writes up to `len` bytes of the `TsmInfo` structure to the non-confidential physical address
    /// `dest_addr`. Returns the number of bytes written.
    ///
    /// a6 = 0
    GetTsmInfo {
        /// a0 = destination address of the `TsmInfo` structure
        dest_addr: u64,
        /// a1 = maximum number of bytes to be written
        len: u64,
    },
    /// Converts `num_pages` of 4kB page-size non-confidential memory starting at `page_addr`. The converted pages
    /// remain non-confidential, and thus may not be assigned for use by a child TVM, until the
    /// fence procedure, described below, has been completed.
    ///
    /// a6 = 1
    ConvertPages {
        /// a0 = base address of pages to convert
        page_addr: u64,
        /// a1 = number of pages
        num_pages: u64,
    },
    /// Reclaims `num_pages` of 4kB page-size confidential memory starting at `page_addr`. The pages must not
    /// be currently assigned to an active TVM.
    ///
    /// a6 = 2
    ReclaimPages {
        /// a0 = base address of pages to reclaim
        page_addr: u64,
        /// a1 = number of pages
        num_pages: u64,
    },
    /// Initiates a TLB invalidation sequence for all pages marked for conversion via calls to
    /// `ConvertPages` between the previous `InitiateGlobalFence` and now. The TLB invalidation
    /// sequence is completed when `LocalFence` has been invoked on all other CPUs, after which
    /// the pages covered by the invalidation sequence are considered to be fully converted &
    /// confidential, and may be assigned for use by child TVMs. An error is returned if a TLB
    /// invalidation sequence is already in progress.
    ///
    /// a6 = 3
    InitiateGlobalFence,
    /// Invalidates TLB entries for all pages pending conversion by an in-progress TLB invalidation
    /// operation on the local CPU.
    ///
    /// a6 = 4
    LocalFence,
    /// Creates a TVM from the parameters in the `TvmCreateParams` structure at the non-confidential
    /// physical address `params_addr`. Returns an opaque TVM handle (encoding a generation counter)
    /// that can be used to refer to the TVM in TVM management TEECALLs.
    ///
    /// a6 = 5
    CreateTvm {
        /// a0 = base physical address of the `TvmCreateParams` structure
        params_addr: u64,
        /// a1 = length of the `TvmCreateParams` structure in bytes
        len: u64,
    },
    /// Moves a VM from the "Initializing" state to the "Runnable" state, finalizing the
    /// measurement of the TVM's configuration and initial memory contents. Sets the initial
    /// entry point (SEPC and opaque argument passed in A1) for the boot vCPU of the TVM.
    ///
    /// a6 = 6
    FinalizeTvm {
        /// a0 = guest_id: opaque TVM handle encoding a generation counter, as returned by `CreateTvm`
        guest_id: u64,
        /// a1 = entry SEPC
        entry_sepc: u64,
        /// a2 = entry argument (A1)
        entry_arg: u64,
        /// a3 = pointer to a 64-byte host-defined TVM identity, or 0 to omit
        /// the `tvm-identity` claim from the attestation token (spec §10.8).
        /// Non-zero values must be 64-byte aligned. This TSM currently has no
        /// attestation token to write the identity into, so a non-zero value
        /// is validated for alignment and then dropped with a diagnostic
        /// line; the field is retained on the wire so hosts can be updated
        /// once attestation lands without another ABI break.
        tvm_identity_addr: u64,
    },
    /// Promote a standard VM to a TVM (not supported in this PoC).
    ///
    /// a6 = 7
    PromoteToTvm {
        /// a0 = guest_id (TVM handle)
        guest_id: u64,
    },
    /// Message to destroy a TVM created with `CreateTvm`.
    ///
    /// a6 = 8
    DestroyTvm {
        /// a0 = guest_id: opaque TVM handle encoding a generation counter, returned from `CreateTvm`.
        guest_id: u64,
    },
    /// Adds a memory region to the TVM identified by `guest_id` at the specified range of guest
    /// physical address space. The memory range is confidential to the guest and may only be
    /// populated with confidential pages. The guest may later convert parts of this region to
    /// shared memory with the `ShareMemory` COVE-Guest call.
    ///
    /// Both `addr` and `len` must be 4kB-aligned and must not overlap with any previously-added
    /// regions. Memory regions may only be added prior to TVM finalization.
    ///
    /// a6 = 9
    AddTvmMemoryRegion {
        /// a0 = guest_id: opaque TVM handle encoding a generation counter, as returned by `CreateTvm`
        guest_id: u64,
        /// a1 = start of the region
        guest_addr: u64,
        /// a2 = length of the region
        len: u64,
    },
    /// Adds `num_pages` 4kB pages of confidential memory starting at `page_addr` to the page-table
    /// page pool for the specified guest.
    ///
    /// a6 = 10
    AddTvmPageTablePages {
        /// a0 = guest_id: opaque TVM handle encoding a generation counter, as returned by `CreateTvm`
        guest_id: u64,
        /// a1 = address of the first page
        page_addr: u64,
        /// a2 = number of pages
        num_pages: u64,
    },
    /// Copies `num_pages` pages from non-confidential memory at `src_addr` to confidential
    /// memory at `dest_addr`, then maps the pages at `dest_addr` into the specified
    /// guest's address space at `guest_addr`. The mapping must lie within a region of confidential
    /// memory created with `AddTvmMemoryRegion`. Pages may only be added prior to TVM
    /// finalization. No cryptographic measurement is computed in this open-source edition.
    ///
    /// a6 = 11
    AddTvmMeasuredPages {
        /// a0 = guest_id: opaque TVM handle encoding a generation counter, as returned by `CreateTvm`
        guest_id: u64,
        /// a1 = physical address of the pages to copy from
        src_addr: u64,
        /// a2 = physical address of the pages to insert
        dest_addr: u64,
        /// a3 = page size
        page_type: TsmPageType,
        /// a4 = number of pages
        num_pages: u64,
        /// a5 = guest physical address
        guest_addr: u64,
    },
    /// Maps `num_pages` zero-filled pages of confidential memory starting at `page_addr` into the
    /// specified guest's address space at `guest_addr`. The mapping must lie within a region of
    /// confidential memory created with `AddTvmMemoryRegion`. Zero pages may only be added after
    /// the TVM has been finalized.
    ///
    /// a6 = 12
    AddTvmZeroPages {
        /// a0 = guest_id: opaque TVM handle encoding a generation counter, as returned by `CreateTvm`
        guest_id: u64,
        /// a1 = physical address of the pages to insert
        page_addr: u64,
        /// a2 = page size
        page_type: TsmPageType,
        /// a3 = number of pages
        num_pages: u64,
        /// a4 = guest physical address
        guest_addr: u64,
    },
    /// Maps non-confidential shared pages in a region of shared memory previously registered by
    /// the guest via `ShareMemory` in the COVE-Guest API.
    ///
    /// a6 = 13
    AddTvmSharedPages {
        /// a0 = guest_id: opaque TVM handle encoding a generation counter, as returned by `CreateTvm`
        guest_id: u64,
        /// a1 = start of the shared memory region
        page_addr: u64,
        /// a2 = page size (must be Page4k for now)
        page_type: TsmPageType,
        /// a3 = number of pages
        num_pages: u64,
        /// a4 = guest physical address
        guest_addr: u64,
    },
    /// Adds a vCPU with ID `vcpu_id` to the guest `guest_id`, using the memory at `state_page_addr`
    /// for internal storage of the vCPU's state.
    ///
    /// `state_page_addr` must be page-aligned and point to a confidential memory region used to hold
    /// the TVM's vCPU state. Must be `TsmInfo::tvm_vcpu_state_pages` pages in length.
    ///
    /// vCPUs may not be added after the TVM is finalized.
    ///
    /// a6 = 14
    CreateTvmVcpu {
        /// a0 = guest_id: opaque TVM handle encoding a generation counter, as returned by `CreateTvm`
        guest_id: u64,
        /// a1 = vCPU id
        vcpu_id: u64,
        /// a2 = address of the first page donated for the vCPU state
        state_page_addr: u64,
    },
    /// Runs the given vCPU in the TVM
    ///
    /// Returns 0 if the vCPU can be resumed via a subsequent call to `RunTvmVcpu`, or a value other
    /// than 0 if the vCPU was terminated and is no longer runnable. Writes SCAUSE with the exit cause
    /// of the vCPU, along with STVAL if appropriate.
    ///
    /// The TSM will update the currently-registered `Shmem` struct with additional details
    /// depending on the exit cause. The layout of the scratch space in the `Shmem` structure is
    /// that of the `TsmShmemScratch` struct defined above.
    ///   - HTVAL is written on guest page faults taken by a TVM vCPU. If the fault is serviceable
    ///     by the host HTVAL is written with bits 63:2 of the faulting guest physical address,
    ///     otherwise 0 is written.
    ///   - HTINST is written on guest page faults taken by a TVM vCPU. If the fault is in an emulated
    ///     MMIO region HTINST is written with a (transformed version of) the faulting load/store
    ///     instruction made by the TVM, otherwise 0 is written. If the faulting instruction is a
    ///     a store, the TSM will also write the value stored by the TVM to the register in
    ///     `guest_gprs` corresponding to the `rs2` register in the instruction. If the faulting
    ///     instruction is a load, the TSM will read from the register in `guest_gprs` corresponding
    ///     to the `rd` register in the instruction and use it to complete the load the next time the
    ///     TVM vCPU is run.
    ///   - VSTIMECMP and VSIE are always written by the TSM upon return from `RunTvmVcpu`.
    ///   - A0-A7 will be written in `guest_gprs` with the ECALL arguments on ECALLs made by the
    ///     TVM vCPU.
    ///
    /// Returns an error if:
    ///   - the specified TVM or vCPU does not exist
    ///   - the vCPU exists but is not currently runnable
    ///   - if hardware AIA virtualization is enabled for the TVM and the vCPU is not bound to
    ///     this physical CPU
    ///
    /// a6 = 15
    RunTvmVcpu {
        /// a0 = guest_id: opaque TVM handle encoding a generation counter, as returned by `CreateTvm`
        guest_id: u64,
        /// a1 = vCPU id
        vcpu_id: u64,
    },
    /// Initiates a TLB invalidation sequence for all pages that have been invalidated in the
    /// given TVM's address space since the previous call to `InitiateTvmFence`. The TLB
    /// invalidation sequence is completed when all vCPUs in the TVM that were running prior to
    /// to the call to `InitiateTvmFence` have taken a trap into the TSM, which the host can
    /// cause by IPI'ing the physical CPUs on which the TVM's vCPUs are running. An error is
    /// returned if a TLB invalidation sequence is already in progress for the TVM.
    ///
    /// a6 = 16
    InitiateTvmFence {
        /// a0 = guest_id: opaque TVM handle encoding a generation counter, as returned by `CreateTvm`
        guest_id: u64,
    },
    /// Invalidates the pages in the specified range of guest physical address space.
    ///
    /// For each page in the range, the TSM verifies that:
    ///   - The page is currently marked present in the TVM’s page table.
    ///   - The page is in either the “Mapped” state and uniquely owned by the TVM, or in
    ///     the “Shared” state and owned by the host.
    ///
    /// After verifying these pre-conditions are met, the TSM:
    ///   - Invalidates the page.
    ///   - Places the page in the “Blocked” state (if “Mapped”) or “BlockedShared” state
    ///     (if “Shared”) and records the TVM TLB version at which the page was invalidated.
    ///     If the page was “Shared”, the reference count of the page is recorded as well.
    ///
    /// Guest page faults taken by the TVM on blocked pages continue to be reported to the
    /// host. The page remains invalid until the mapping is unblocked (marked present),
    /// removed, or part of a huge page promotion/demotion operation.
    ///
    /// Returns 0 on success.
    ///
    /// a6 = 17
    TvmInvalidatePages {
        /// a0 = guest_id: opaque TVM handle encoding a generation counter, as returned by `CreateTvm`
        guest_id: u64,
        /// a1 = guest physical address
        guest_addr: u64,
        /// a2 = length of the range
        len: u64,
    },
    /// Marks the invalidated pages in the specified range of guest physical address space
    /// as present.
    ///
    /// For each page in the range, the TSM verifies that:
    ///   - The page is currently marked invalid in the TVM’s page table.
    ///   - The page is in either the “Blocked” state and uniquely owned by the TVM, or in
    ///     the “BlockedShared” state and owned by the host.
    ///
    /// After verifying these pre-conditions are met, the TSM:
    ///   - Marks the page as present.
    ///   - Places the page in the “Mapped” state (if “Blocked”) or “Shared” state
    ///     (if “BlockedShared”). If the page was “BlockedShared”, the reference count
    ///     of the page is restored.
    ///
    /// Unblock may be used to revert an in-progress page removal or huge page
    /// promotion/demotion sequence.
    ///
    /// Returns 0 on success.
    ///
    /// a6 = 18
    TvmValidatePages {
        /// a0 = guest_id: opaque TVM handle encoding a generation counter, as returned by `CreateTvm`
        guest_id: u64,
        /// a1 = guest physical address
        guest_addr: u64,
        /// a2 = length of the range
        len: u64,
    },
    /// Removes mappings from a TVM. The range to be unmapped must already have been invalidated
    /// and fenced, and must lie within a removable region of guest physical address space.
    ///
    /// A region of guest physical address space is in a removable state if the guest previously
    /// invoked ShareMemory or UnshareMemory from the COVE-Guest interface, and that it marked the
    /// region as "ConfidentialRemovable" or "SharedRemovable".
    ///
    /// The TSM verifies that:
    ///   - All pages within the requested range are invalidated, and are in the “Blocked” or
    ///     “BlockedShared” state.
    ///   - The TLB version at which the pages were invalidated is older than the TVM’s current
    ///     TLB version.
    ///   - The address range lies within a removable (“SharedRemovable” or “ConfidentialRemovable”)
    ///     region of guest physical address space.
    ///
    /// After verifying these pre-conditions are met, the TSM:
    ///   - Clears (zeros out) all PTEs within the specified range.
    ///   - Places each page in either the “Converted” state, with ownership returned to the host
    ///     (if previously “Blocked”) or the “Shared” state, with the reference count decremented
    ///     (if previously “BlockedShared”).
    ///
    /// Returns 0 on success.
    ///
    /// a6 = 19
    TvmRemovePages {
        /// a0 = guest_id: opaque TVM handle encoding a generation counter, as returned by `CreateTvm`
        guest_id: u64,
        /// a1 = guest physical address
        guest_addr: u64,
        /// a2 = length of the range
        len: u64,
    },
    /// Converts `num_pages` of 4kB non-confidential pages starting at `page_addr` for use as
    /// TVM-shared memory. Same argument layout as `ConvertPages`, but the host retains access
    /// to the pages: the RDSM deliberately skips revoking the host's MPT permissions for this
    /// funcid. The TSM must therefore track these pages as shared rather than confidential.
    ///
    /// a6 = 20
    ConvertSharedPages {
        /// a0 = base address of pages to convert
        page_addr: u64,
        /// a1 = number of pages
        num_pages: u64,
    },
}

impl CoveHostFunction {
    /// Attempts to parse `Self` from the passed in `a0-a7`.
    pub(crate) fn from_regs(args: &[u64]) -> Result<Self> {
        use CoveHostFunction::*;
        match args[6] {
            0 => Ok(GetTsmInfo {
                dest_addr: args[0],
                len: args[1],
            }),
            1 => Ok(ConvertPages {
                page_addr: args[0],
                num_pages: args[1],
            }),
            2 => Ok(ReclaimPages {
                page_addr: args[0],
                num_pages: args[1],
            }),
            3 => Ok(InitiateGlobalFence),
            4 => Ok(LocalFence),
            5 => Ok(CreateTvm {
                params_addr: args[0],
                len: args[1],
            }),
            6 => Ok(FinalizeTvm {
                guest_id: args[0],
                entry_sepc: args[1],
                entry_arg: args[2],
                tvm_identity_addr: args[3],
            }),
            7 => Ok(PromoteToTvm { guest_id: args[0] }),
            8 => Ok(DestroyTvm { guest_id: args[0] }),
            9 => Ok(AddTvmMemoryRegion {
                guest_id: args[0],
                guest_addr: args[1],
                len: args[2],
            }),
            10 => Ok(AddTvmPageTablePages {
                guest_id: args[0],
                page_addr: args[1],
                num_pages: args[2],
            }),
            11 => Ok(AddTvmMeasuredPages {
                guest_id: args[0],
                src_addr: args[1],
                dest_addr: args[2],
                page_type: TsmPageType::from_reg(args[3])?,
                num_pages: args[4],
                guest_addr: args[5],
            }),
            12 => Ok(AddTvmZeroPages {
                guest_id: args[0],
                page_addr: args[1],
                page_type: TsmPageType::from_reg(args[2])?,
                num_pages: args[3],
                guest_addr: args[4],
            }),
            13 => Ok(AddTvmSharedPages {
                guest_id: args[0],
                page_addr: args[1],
                page_type: TsmPageType::from_reg(args[2])?,
                num_pages: args[3],
                guest_addr: args[4],
            }),
            14 => Ok(CreateTvmVcpu {
                guest_id: args[0],
                vcpu_id: args[1],
                state_page_addr: args[2],
            }),
            15 => Ok(RunTvmVcpu {
                guest_id: args[0],
                vcpu_id: args[1],
            }),
            16 => Ok(InitiateTvmFence { guest_id: args[0] }),
            17 => Ok(TvmInvalidatePages {
                guest_id: args[0],
                guest_addr: args[1],
                len: args[2],
            }),
            18 => Ok(TvmValidatePages {
                guest_id: args[0],
                guest_addr: args[1],
                len: args[2],
            }),
            19 => Ok(TvmRemovePages {
                guest_id: args[0],
                guest_addr: args[1],
                len: args[2],
            }),
            // funcid 20 = CONVERT_SHARED_PAGES: same args as CONVERT_PAGES, but the host keeps
            // its MPT access, so the TSM must track the pages as shared, not confidential.
            20 => Ok(ConvertSharedPages {
                page_addr: args[0],
                num_pages: args[1],
            }),
            _ => Err(Error::NotSupported),
        }
    }
}

impl SbiFunction for CoveHostFunction {
    fn a6(&self) -> u64 {
        use CoveHostFunction::*;
        match self {
            GetTsmInfo {
                dest_addr: _,
                len: _,
            } => 0,
            ConvertPages {
                page_addr: _,
                num_pages: _,
            } => 1,
            ReclaimPages {
                page_addr: _,
                num_pages: _,
            } => 2,
            InitiateGlobalFence => 3,
            LocalFence => 4,
            CreateTvm {
                params_addr: _,
                len: _,
            } => 5,
            FinalizeTvm {
                guest_id: _,
                entry_sepc: _,
                entry_arg: _,
                tvm_identity_addr: _,
            } => 6,
            PromoteToTvm { guest_id: _ } => 7,
            DestroyTvm { guest_id: _ } => 8,
            AddTvmMemoryRegion {
                guest_id: _,
                guest_addr: _,
                len: _,
            } => 9,
            AddTvmPageTablePages {
                guest_id: _,
                page_addr: _,
                num_pages: _,
            } => 10,
            AddTvmMeasuredPages {
                guest_id: _,
                src_addr: _,
                dest_addr: _,
                page_type: _,
                num_pages: _,
                guest_addr: _,
            } => 11,
            AddTvmZeroPages {
                guest_id: _,
                page_addr: _,
                page_type: _,
                num_pages: _,
                guest_addr: _,
            } => 12,
            AddTvmSharedPages {
                guest_id: _,
                page_addr: _,
                page_type: _,
                num_pages: _,
                guest_addr: _,
            } => 13,
            CreateTvmVcpu {
                guest_id: _,
                vcpu_id: _,
                state_page_addr: _,
            } => 14,
            RunTvmVcpu {
                guest_id: _,
                vcpu_id: _,
            } => 15,
            InitiateTvmFence { guest_id: _ } => 16,
            TvmInvalidatePages {
                guest_id: _,
                guest_addr: _,
                len: _,
            } => 17,
            TvmValidatePages {
                guest_id: _,
                guest_addr: _,
                len: _,
            } => 18,
            TvmRemovePages {
                guest_id: _,
                guest_addr: _,
                len: _,
            } => 19,
            ConvertSharedPages {
                page_addr: _,
                num_pages: _,
            } => 20,
        }
    }

    fn a0(&self) -> u64 {
        use CoveHostFunction::*;
        match self {
            CreateTvm {
                params_addr,
                len: _,
            } => *params_addr,
            DestroyTvm { guest_id } => *guest_id,
            PromoteToTvm { guest_id } => *guest_id,
            AddTvmPageTablePages {
                guest_id,
                page_addr: _,
                num_pages: _,
            } => *guest_id,
            AddTvmZeroPages {
                guest_id,
                page_addr: _,
                page_type: _,
                num_pages: _,
                guest_addr: _,
            } => *guest_id,
            FinalizeTvm {
                guest_id,
                entry_sepc: _,
                entry_arg: _,
                tvm_identity_addr: _,
            } => *guest_id,
            RunTvmVcpu {
                guest_id,
                vcpu_id: _,
            } => *guest_id,
            CreateTvmVcpu {
                guest_id,
                vcpu_id: _,
                state_page_addr: _,
            } => *guest_id,
            GetTsmInfo { dest_addr, len: _ } => *dest_addr,
            AddTvmMeasuredPages {
                guest_id,
                src_addr: _,
                dest_addr: _,
                page_type: _,
                num_pages: _,
                guest_addr: _,
            } => *guest_id,
            ConvertPages {
                page_addr,
                num_pages: _,
            } => *page_addr,
            ReclaimPages {
                page_addr,
                num_pages: _,
            } => *page_addr,
            AddTvmMemoryRegion {
                guest_id,
                guest_addr: _,
                len: _,
            } => *guest_id,
            AddTvmSharedPages {
                guest_id,
                page_addr: _,
                page_type: _,
                num_pages: _,
                guest_addr: _,
            } => *guest_id,
            InitiateTvmFence { guest_id } => *guest_id,
            TvmInvalidatePages {
                guest_id,
                guest_addr: _,
                len: _,
            } => *guest_id,
            TvmValidatePages {
                guest_id,
                guest_addr: _,
                len: _,
            } => *guest_id,
            TvmRemovePages {
                guest_id,
                guest_addr: _,
                len: _,
            } => *guest_id,
            ConvertSharedPages {
                page_addr,
                num_pages: _,
            } => *page_addr,
            _ => 0,
        }
    }

    fn a1(&self) -> u64 {
        use CoveHostFunction::*;
        match self {
            CreateTvm {
                params_addr: _,
                len,
            } => *len,
            AddTvmPageTablePages {
                guest_id: _,
                page_addr,
                num_pages: _,
            } => *page_addr,
            AddTvmZeroPages {
                guest_id: _,
                page_addr,
                page_type: _,
                num_pages: _,
                guest_addr: _,
            } => *page_addr,
            FinalizeTvm {
                guest_id: _,
                entry_sepc,
                entry_arg: _,
                tvm_identity_addr: _,
            } => *entry_sepc,
            RunTvmVcpu {
                guest_id: _,
                vcpu_id,
            } => *vcpu_id,
            CreateTvmVcpu {
                guest_id: _,
                vcpu_id,
                state_page_addr: _,
            } => *vcpu_id,
            GetTsmInfo { dest_addr: _, len } => *len,
            AddTvmMeasuredPages {
                guest_id: _,
                src_addr,
                dest_addr: _,
                page_type: _,
                num_pages: _,
                guest_addr: _,
            } => *src_addr,
            ConvertPages {
                page_addr: _,
                num_pages,
            } => *num_pages,
            ReclaimPages {
                page_addr: _,
                num_pages,
            } => *num_pages,
            AddTvmMemoryRegion {
                guest_id: _,
                guest_addr,
                len: _,
            } => *guest_addr,
            AddTvmSharedPages {
                guest_id: _,
                page_addr,
                page_type: _,
                num_pages: _,
                guest_addr: _,
            } => *page_addr,
            TvmInvalidatePages {
                guest_id: _,
                guest_addr,
                len: _,
            } => *guest_addr,
            TvmValidatePages {
                guest_id: _,
                guest_addr,
                len: _,
            } => *guest_addr,
            TvmRemovePages {
                guest_id: _,
                guest_addr,
                len: _,
            } => *guest_addr,
            ConvertSharedPages {
                page_addr: _,
                num_pages,
            } => *num_pages,
            _ => 0,
        }
    }

    fn a2(&self) -> u64 {
        use CoveHostFunction::*;
        match self {
            AddTvmPageTablePages {
                guest_id: _,
                page_addr: _,
                num_pages,
            } => *num_pages,
            AddTvmZeroPages {
                guest_id: _,
                page_addr: _,
                page_type,
                num_pages: _,
                guest_addr: _,
            } => *page_type as u64,
            FinalizeTvm {
                guest_id: _,
                entry_sepc: _,
                entry_arg,
                tvm_identity_addr: _,
            } => *entry_arg,
            CreateTvmVcpu {
                guest_id: _,
                vcpu_id: _,
                state_page_addr,
            } => *state_page_addr,
            AddTvmMeasuredPages {
                guest_id: _,
                src_addr: _,
                dest_addr,
                page_type: _,
                num_pages: _,
                guest_addr: _,
            } => *dest_addr,
            AddTvmMemoryRegion {
                guest_id: _,
                guest_addr: _,
                len,
            } => *len,
            AddTvmSharedPages {
                guest_id: _,
                page_addr: _,
                page_type,
                num_pages: _,
                guest_addr: _,
            } => *page_type as u64,
            TvmInvalidatePages {
                guest_id: _,
                guest_addr: _,
                len,
            } => *len,
            TvmValidatePages {
                guest_id: _,
                guest_addr: _,
                len,
            } => *len,
            TvmRemovePages {
                guest_id: _,
                guest_addr: _,
                len,
            } => *len,
            _ => 0,
        }
    }

    fn a3(&self) -> u64 {
        use CoveHostFunction::*;
        match self {
            FinalizeTvm {
                guest_id: _,
                entry_sepc: _,
                entry_arg: _,
                tvm_identity_addr,
            } => *tvm_identity_addr,
            AddTvmZeroPages {
                guest_id: _,
                page_addr: _,
                page_type: _,
                num_pages,
                guest_addr: _,
            } => *num_pages,
            AddTvmMeasuredPages {
                guest_id: _,
                src_addr: _,
                dest_addr: _,
                page_type,
                num_pages: _,
                guest_addr: _,
            } => *page_type as u64,
            AddTvmSharedPages {
                guest_id: _,
                page_addr: _,
                page_type: _,
                num_pages,
                guest_addr: _,
            } => *num_pages,
            _ => 0,
        }
    }

    fn a4(&self) -> u64 {
        use CoveHostFunction::*;
        match self {
            AddTvmZeroPages {
                guest_id: _,
                page_addr: _,
                page_type: _,
                num_pages: _,
                guest_addr,
            } => *guest_addr,
            AddTvmMeasuredPages {
                guest_id: _,
                src_addr: _,
                dest_addr: _,
                page_type: _,
                num_pages,
                guest_addr: _,
            } => *num_pages,
            AddTvmSharedPages {
                guest_id: _,
                page_addr: _,
                page_type: _,
                num_pages: _,
                guest_addr,
            } => *guest_addr,
            _ => 0,
        }
    }

    fn a5(&self) -> u64 {
        use CoveHostFunction::*;
        match self {
            AddTvmMeasuredPages {
                guest_id: _,
                src_addr: _,
                dest_addr: _,
                page_type: _,
                num_pages: _,
                guest_addr,
            } => *guest_addr,
            _ => 0,
        }
    }
}
