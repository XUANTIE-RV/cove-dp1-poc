// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! Per-physical-page ownership and state tracking.
//!
//! CoVE gives the TSM sole responsibility for isolating confidential memory
//! *between* TVMs: the RDSM's MPT only distinguishes the host domain from the
//! confidential domain, so every TVM and the TSM itself share one SDID and the
//! MPT cannot express which TVM a page belongs to. G-stage page tables are the
//! only enforcement point, and a page table alone cannot answer "is this page
//! already owned by somebody else?" — hence this table.
//!
//! One byte per 4KB page: the low nibble is a [`PageState`], the high nibble is
//! the owning guest_id (or [`NO_OWNER`]). An all-zero byte therefore means
//! "non-confidential, unowned", which is the correct fail-closed default: a page
//! nobody has converted yet is treated as belonging to the host, so handing it
//! to a TVM requires an explicit conversion first.
//!
//! This module is deliberately free of target gating and of any global state so
//! that the transition rules can be exercised by host unit tests; the backing
//! array and the call sites live in the riscv64-only paths.

use config::{PAGE_SIZE, RAM_START, TRACKED_PAGES, TRACKED_RAM_SIZE};

/// Owner nibble value meaning "no TVM owns this page".
///
/// Owners are stored biased by one (`gid + 1`) so that the zero nibble of a
/// zero-initialized table decodes to "unowned". Storing the raw guest id would
/// make an all-zero byte read back as "owned by guest 0", which is a valid id
/// and therefore not a usable sentinel.
pub const NO_OWNER: u8 = 0;

/// Encode a guest id into its owner nibble.
#[inline]
pub const fn owner_of_guest(guest_id: u8) -> u8 {
    guest_id + 1
}

/// Decode an owner nibble back to a guest id, or `None` when unowned.
#[inline]
pub const fn guest_of_owner(owner: u8) -> Option<u8> {
    if owner == NO_OWNER {
        None
    } else {
        Some(owner - 1)
    }
}

/// Lifecycle state of a single physical page.
///
/// Discriminants are stable because they are stored in [`PAGE_META`]-style byte
/// arrays; `NonConfidential` must stay 0 so zeroed memory decodes to it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum PageState {
    /// Host-owned, not confidential. The zero-initialized default.
    NonConfidential = 0,
    /// Conversion accepted but the TLB invalidation sequence has not finished.
    ///
    /// Reserved for the fence protocol; nothing parks pages here yet.
    InTransition = 1,
    /// Confidential and not assigned to any TVM.
    ConfUnassigned = 2,
    /// Confidential and mapped into the owning TVM's G-stage table.
    ConfAssigned = 3,
    /// Invalidated and awaiting a fence before it may be removed.
    ///
    /// Reserved for the fence protocol.
    ConfBlocked = 4,
    /// Non-confidential but mapped into the owning TVM, i.e. host-shared memory.
    Shared = 5,
    /// Used by the TSM on the owning TVM's behalf: G-stage root, page table
    /// pool pages, vCPU state pages. Never mapped into the TVM itself.
    TsmPrivate = 6,
    /// A `Shared` page that has been invalidated and awaits removal.
    ///
    /// Distinct from [`PageState::ConfBlocked`] because unblocking has to restore
    /// the page to whichever state it came from, and the host blocks shared pages
    /// too (that is how a TVM un-shares memory).
    SharedBlocked = 7,
    /// The metadata byte held an encoding this build does not define.
    ///
    /// Only reachable through memory corruption. It exists because falling back
    /// to `NonConfidential` would be fail-*open*: that state is the one every
    /// conversion accepts and the only one reclaim may skip scrubbing, so a
    /// corrupted byte would read back as "host may take this page, uncleaned".
    /// Every operation refuses `Corrupted`, and reclaiming it still scrubs.
    Corrupted = 8,
}

impl PageState {
    /// Decode a state nibble.
    ///
    /// Encodings this build does not define resolve to [`PageState::Corrupted`],
    /// which no operation accepts.
    pub const fn from_nibble(nibble: u8) -> Self {
        match nibble & 0xF {
            0 => PageState::NonConfidential,
            1 => PageState::InTransition,
            2 => PageState::ConfUnassigned,
            3 => PageState::ConfAssigned,
            4 => PageState::ConfBlocked,
            5 => PageState::Shared,
            6 => PageState::TsmPrivate,
            7 => PageState::SharedBlocked,
            _ => PageState::Corrupted,
        }
    }

    /// Whether a page in this state is currently claimed by a TVM.
    ///
    /// Reclaiming such a page would hand the host a frame that is still mapped
    /// into a live TVM's G-stage table.
    ///
    /// `Shared` counts as claimed: it carries an owner and is mapped into that
    /// TVM, so releasing it would let the host scrub a buffer the guest is
    /// actively using and would clear the owner, after which the TVM's own
    /// block/remove calls fail on ownership and the mapping can never be torn
    /// down. `Corrupted` counts too, because nothing may be assumed about it.
    pub const fn is_claimed(self) -> bool {
        matches!(
            self,
            PageState::ConfAssigned
                | PageState::ConfBlocked
                | PageState::Shared
                | PageState::SharedBlocked
                | PageState::TsmPrivate
                | PageState::Corrupted
        )
    }
}

/// Pack a state and owner into a metadata byte.
#[inline]
pub const fn pack(state: PageState, owner: u8) -> u8 {
    ((owner & 0xF) << 4) | (state as u8 & 0xF)
}

/// Extract the state from a metadata byte.
#[inline]
pub const fn state_of(meta: u8) -> PageState {
    PageState::from_nibble(meta)
}

/// Extract the owner nibble from a metadata byte.
#[inline]
pub const fn owner_of(meta: u8) -> u8 {
    meta >> 4
}

/// Convert a physical address to its index in the metadata array.
///
/// Returns `None` for addresses outside the tracked RAM window — device memory
/// such as IMSIC pages, which the state machine treats as permanently
/// non-confidential rather than rejecting outright.
#[inline]
pub const fn page_index(pa: u64) -> Option<usize> {
    if pa < RAM_START {
        return None;
    }
    let offset = pa - RAM_START;
    if offset >= TRACKED_RAM_SIZE {
        return None;
    }
    Some((offset / PAGE_SIZE) as usize)
}

/// Whether a physical address falls inside the tracked RAM window.
#[inline]
pub const fn is_tracked(pa: u64) -> bool {
    page_index(pa).is_some()
}

/// Validate a host-supplied page range without overflowing.
///
/// `first_pa` and `num_pages` both come straight from the untrusted host, and
/// walking the range as `first_pa + i * PAGE_SIZE` would wrap for an absurd
/// `num_pages`. A wrapped address can land back inside the tracked window and
/// pass checks it should not, so the range is validated as a whole up front.
///
/// Ranges that fall outside the tracked window are still accepted here; whether
/// device memory is legal is the caller's decision, since it depends on the
/// operation.
pub fn validate_range(first_pa: u64, num_pages: u64) -> Result<(), TransitionError> {
    if first_pa % PAGE_SIZE != 0 {
        return Err(TransitionError::Untracked);
    }
    let span = num_pages
        .checked_mul(PAGE_SIZE)
        .ok_or(TransitionError::Untracked)?;
    first_pa
        .checked_add(span)
        .ok_or(TransitionError::Untracked)?;
    Ok(())
}

/// Like [`validate_range`], but additionally requires every page to be inside
/// the tracked RAM window.
///
/// Use this when the TSM is about to *dereference* the range rather than merely
/// record metadata about it. [`validate_range`] deliberately tolerates addresses
/// outside the window because device memory is legal to share; a caller that is
/// going to write to the range needs the stronger guarantee that the host handed
/// it real, tracked RAM. Without it, a host that under-allocates a control
/// region — or names an address it does not own — gets the TSM to write there on
/// its behalf.
pub fn require_tracked_range(first_pa: u64, num_pages: u64) -> Result<(), TransitionError> {
    validate_range(first_pa, num_pages)?;
    for i in 0..num_pages {
        let pa = first_pa + i * PAGE_SIZE;
        if page_index(pa).is_none() {
            return Err(TransitionError::Untracked);
        }
    }
    Ok(())
}

/// The operation a caller wants to perform on a page.
///
/// Each variant corresponds to one COVH call that moves pages between states.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PageOp {
    /// `ConvertPages` (FID 1): host memory becomes confidential.
    Convert,
    /// `ConvertSharedPages` (FID 21): host keeps access; page becomes TVM-shared.
    ConvertShared,
    /// `AddTvmZeroPages`: assign a scrubbed confidential page to a TVM.
    AddZero,
    /// `AddTvmMeasuredPages`: assign a confidential page holding TVM payload.
    AddMeasured,
    /// `AddTvmPageTablePages`: donate confidential pages to the TSM's G-stage pool.
    AddPageTable,
    /// `AddTvmSharedPages`: map non-confidential memory into a TVM.
    AddShared,
    /// `TvmInvalidatePages` (spec: invalidate): stop TVM access pending removal.
    Block,
    /// `TvmValidatePages` (spec: validate): restore a blocked page.
    Unblock,
    /// `TvmRemovePages`: drop a blocked page's mapping and release ownership.
    Remove,
    /// `ReclaimPages` (FID 2): scrub and hand a page back to the host.
    Reclaim,
}

/// Why a requested page transition was refused.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TransitionError {
    /// The page is not in a state this operation accepts.
    WrongState,
    /// The page belongs to a different TVM than the caller.
    WrongOwner,
    /// The address lies outside the tracked RAM window.
    Untracked,
    /// The supplied guest_id is not valid (single-TVM POC requires 0).
    BadOwner,
    /// The G-stage page-table pool had no free page to allocate an
    /// intermediate level. Reported when `map_page` couldn't complete the
    /// walk; the host donated fewer pages via `AddTvmPageTablePages` than the
    /// TVM's mapping activity requires. Kept out of the state-transition
    /// vocabulary above because the failure is orthogonal to the page's
    /// metadata; distinguishing it lets diagnostics point at the pool
    /// exhaustion cause and keeps the SBI mapping (→ `SBI_ERR_FAILED`) at
    /// the caller.
    PgtPoolExhausted,
}

/// Decide whether `op` may be applied to a page whose metadata is `meta`.
///
/// On success returns the metadata byte the page should carry afterwards, so the
/// caller has a single source of truth for both the check and the update.
///
/// `requester` is an **owner nibble** (see [`owner_of_guest`]), or [`NO_OWNER`]
/// for the TVM-independent conversions and reclaims.
pub fn next_meta(meta: u8, op: PageOp, requester: u8) -> Result<u8, TransitionError> {
    let state = state_of(meta);
    let owner = owner_of(meta);

    // Operations that attach a page to a TVM need a valid owner to attach it to.
    let needs_owner = matches!(
        op,
        PageOp::AddZero
            | PageOp::AddMeasured
            | PageOp::AddPageTable
            | PageOp::AddShared
            | PageOp::Block
            | PageOp::Unblock
            | PageOp::Remove
    );
    if needs_owner {
        match guest_of_owner(requester) {
            Some(gid) if (gid as usize) == 0 => {}
            _ => return Err(TransitionError::BadOwner),
        }
    }

    match op {
        PageOp::Convert => match state {
            PageState::NonConfidential => Ok(pack(PageState::ConfUnassigned, NO_OWNER)),
            _ => Err(TransitionError::WrongState),
        },
        PageOp::ConvertShared => match state {
            // Conversion is TVM-independent, so the page comes out unowned. It
            // must not inherit the byte's existing owner nibble: assignment is
            // the job of `AddShared`, and inheriting would leave a freshly
            // converted page looking like it already belongs to someone.
            PageState::NonConfidential => Ok(pack(PageState::Shared, NO_OWNER)),
            // Idempotent only while still unowned; once a TVM has it, a second
            // conversion would silently strip that ownership.
            PageState::Shared if owner == NO_OWNER => Ok(pack(PageState::Shared, NO_OWNER)),
            PageState::Shared => Err(TransitionError::WrongOwner),
            _ => Err(TransitionError::WrongState),
        },
        PageOp::AddZero | PageOp::AddMeasured => match state {
            PageState::ConfUnassigned => Ok(pack(PageState::ConfAssigned, requester)),
            _ => Err(TransitionError::WrongState),
        },
        PageOp::AddPageTable => match state {
            PageState::ConfUnassigned => Ok(pack(PageState::TsmPrivate, requester)),
            _ => Err(TransitionError::WrongState),
        },
        PageOp::AddShared => match state {
            PageState::NonConfidential => Ok(pack(PageState::Shared, requester)),
            // Re-mapping a page this TVM already shares is fine; stealing one
            // that another TVM shares is not.
            PageState::Shared if owner == NO_OWNER || owner == requester => {
                Ok(pack(PageState::Shared, requester))
            }
            PageState::Shared => Err(TransitionError::WrongOwner),
            _ => Err(TransitionError::WrongState),
        },
        PageOp::Block => {
            if owner != requester {
                return Err(TransitionError::WrongOwner);
            }
            match state {
                PageState::ConfAssigned => Ok(pack(PageState::ConfBlocked, owner)),
                PageState::Shared => Ok(pack(PageState::SharedBlocked, owner)),
                // Idempotent: re-blocking an already blocked page is harmless.
                PageState::ConfBlocked | PageState::SharedBlocked => Ok(meta),
                _ => Err(TransitionError::WrongState),
            }
        }
        PageOp::Unblock => {
            if owner != requester {
                return Err(TransitionError::WrongOwner);
            }
            match state {
                PageState::ConfBlocked => Ok(pack(PageState::ConfAssigned, owner)),
                PageState::SharedBlocked => Ok(pack(PageState::Shared, owner)),
                // Idempotent: validating a page that is already live.
                PageState::ConfAssigned | PageState::Shared => Ok(meta),
                _ => Err(TransitionError::WrongState),
            }
        }
        PageOp::Remove => {
            if owner != requester {
                return Err(TransitionError::WrongOwner);
            }
            match state {
                // Confidential pages go back to the pool; the host must still
                // reclaim them before it may touch the memory again.
                PageState::ConfBlocked => Ok(pack(PageState::ConfUnassigned, NO_OWNER)),
                // Shared pages were never confidential, so they return to the host.
                PageState::SharedBlocked => Ok(pack(PageState::NonConfidential, NO_OWNER)),
                _ => Err(TransitionError::WrongState),
            }
        }
        PageOp::Reclaim => {
            if state.is_claimed() {
                // spec FID #2: "pages must not be currently assigned to an
                // active TVM". Returning success here would make the RDSM
                // restore host RWX on a frame a live TVM still has mapped.
                return Err(TransitionError::WrongState);
            }
            Ok(pack(PageState::NonConfidential, NO_OWNER))
        }
    }
}

/// Convert a metadata array index back to the physical address of its page.
///
/// Inverse of [`page_index`]; the destroy path needs it to turn a table scan hit
/// back into an address it can scrub.
#[inline]
pub const fn page_addr(index: usize) -> u64 {
    RAM_START + (index as u64) * PAGE_SIZE
}

/// Apply `op` to the page at `pa` within `table`, updating it in place.
///
/// Takes the table as a parameter rather than reaching for the global array so
/// the state machine can be exercised on the host with a small table.
///
/// Returns the page's new metadata on success.
pub fn apply(table: &mut [u8], pa: u64, op: PageOp, requester: u8) -> Result<u8, TransitionError> {
    let index = page_index(pa).ok_or(TransitionError::Untracked)?;
    let current = *table.get(index).ok_or(TransitionError::Untracked)?;
    let next = next_meta(current, op, requester)?;
    table[index] = next;
    Ok(next)
}

/// Metadata a page should carry once `owner` releases it, or `None` if the page
/// is not held by that owner.
///
/// Drives TVM teardown: every page the destroyed TVM held goes back to
/// `ConfUnassigned` with no owner, so the host can reclaim it afterwards. Pages
/// the TVM merely shared were never confidential, so they return to the host
/// pool directly.
pub fn release_for_owner(meta: u8, owner: u8) -> Option<u8> {
    if owner_of(meta) != owner {
        return None;
    }
    match state_of(meta) {
        PageState::ConfAssigned | PageState::ConfBlocked | PageState::TsmPrivate => {
            Some(pack(PageState::ConfUnassigned, NO_OWNER))
        }
        // Both shared variants were never confidential, so they return to the
        // host pool. `SharedBlocked` must be handled here: a TVM destroyed part
        // way through an un-share sequence would otherwise leave pages stuck in
        // that state carrying a dead owner, and `is_claimed` would then make
        // reclaim refuse them forever.
        PageState::Shared | PageState::SharedBlocked => {
            Some(pack(PageState::NonConfidential, NO_OWNER))
        }
        // Nothing else can legitimately carry an owner nibble, and a corrupted
        // byte gives no basis for deciding where the page should go.
        _ => None,
    }
}

/// The live per-page metadata table.
///
/// Zeroed at load time, which decodes to "non-confidential, unowned" for every
/// page — the fail-closed default. One byte per 4KB page over
/// [`TRACKED_RAM_SIZE`], so 1MB of `.bss`.
///
/// Guarded by a spin lock rather than made of per-page atomics on purpose. These
/// operations are *batches* that must be all-or-nothing: with per-page atomics,
/// two harts could each pass their own validation pass and then both commit, so
/// one physical page would end up mapped into two TVMs — exactly the failure this
/// table exists to prevent. Rolling back a partially committed batch would
/// itself race with whoever else is mutating those pages. Holding one lock
/// across validate-and-commit removes both problems.
///
/// Host unit tests deliberately do not touch this table; they pass their own
/// small slices to [`apply`] instead, so parallel tests cannot interfere.
#[cfg(all(target_arch = "riscv64", target_os = "none"))]
static PAGE_META: utils::sync::Mutex<[u8; TRACKED_PAGES]> =
    utils::sync::Mutex::new([0; TRACKED_PAGES]);

/// Lock the live metadata table for the duration of one batch.
///
/// Callers must complete their validate-and-commit inside the returned guard and
/// only then drop it to do the slow work (scrubbing, mapping, unmapping), which
/// is safe because the committed metadata already reserves those pages.
#[cfg(all(target_arch = "riscv64", target_os = "none"))]
pub fn lock_table() -> utils::sync::MutexGuard<'static, [u8; TRACKED_PAGES]> {
    PAGE_META.lock()
}

// Compile-time guards on the encoding.
const _: () = assert!(NO_OWNER == 0);
const _: () = assert!(TRACKED_PAGES > 0);
