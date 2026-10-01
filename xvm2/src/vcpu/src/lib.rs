// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! vCPU types: lifecycle status, per-vCPU state, and core operations.
//!
//! This crate is `#![no_std]` and can be compiled and unit-tested on any
//! host platform. Architecture-specific register context types
//! (VmCpuRegisters, etc.) are defined in the `riscv-regs` crate.

#![no_std]

pub mod resume;
pub use resume::{ExitKind, PendingResume, ResumeInput};

use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use riscv_regs::{GprIndex, VmCpuRegisters};

// ---------------------------------------------------------------------------
// RISC-V sstatus bit definitions for guest initial state
// ---------------------------------------------------------------------------
const SSTATUS_SPIE: u64 = 1 << 5;
const SSTATUS_SPP: u64 = 1 << 8;
const SSTATUS_FS_INITIAL: u64 = 1 << 13;
const SSTATUS_VS_INITIAL: u64 = 1 << 9;
const SCOUNTEREN_INIT: u64 = 0x7;

/// Initial guest sstatus: SPP=1 (supervisor), SPIE=1, FS=Initial, VS=Initial.
pub const GUEST_INIT_SSTATUS: u64 =
    SSTATUS_SPP | SSTATUS_SPIE | SSTATUS_FS_INITIAL | SSTATUS_VS_INITIAL;

/// Initial guest scounteren: cycle + time + instret.
pub const GUEST_INIT_SCOUNTEREN: u64 = SCOUNTEREN_INIT;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors from vCPU operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VcpuError {
    /// vCPU is not in Runnable state (for activate).
    NotRunnable,
    /// vCPU is not in PoweredOff state (for power_on).
    AlreadyPowered,
}

// ---------------------------------------------------------------------------
// VcpuStatus
// ---------------------------------------------------------------------------

/// 4-state vCPU lifecycle for SBI HSM compliance.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum VcpuStatus {
    PoweredOff = 0,
    Created = 1,
    Runnable = 2,
    Running = 3,
}

impl VcpuStatus {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => VcpuStatus::Created,
            2 => VcpuStatus::Runnable,
            3 => VcpuStatus::Running,
            _ => VcpuStatus::PoweredOff,
        }
    }
}

// ---------------------------------------------------------------------------
// Vcpu
// ---------------------------------------------------------------------------

/// Per-vCPU state: register context, lifecycle status, and per-vCPU resources.
///
/// `status` uses AtomicU8 because HartGetStatus can read any vCPU's status
/// from a different hart concurrently with that vCPU's status being modified.
/// `first_run` uses AtomicBool because it is written by the HartStart issuer
/// (hart A) and read by the target vCPU's hart (hart B) in RunTvmVcpu.
pub struct Vcpu {
    pub regs: VmCpuRegisters,
    pub status: AtomicU8,
    pub first_run: AtomicBool,
    pub vcpu_id: usize,
    pub pcpu_id: usize,
    pub imsic_gpa: u64,
    /// Per-vCPU cached IMSIC hardware address. Populated on first run by
    /// `Tvm::setup_vcpu_imsic` from `Tvm::imsic_hw_by_hart[hart]`; the
    /// interrupt injection path (InjectTvmCpu) reads this
    /// cache directly instead of doing a per-hart table lookup.
    pub imsic_hw_addr: u64,
    /// The TSM's own account of why this vCPU last left the guest.
    ///
    /// Resume decisions are made from this record, never from the exit values
    /// read back out of the shared memory page: that page is host-writable, so trusting
    /// it would let the host name the guest register to overwrite and the
    /// distance to advance `sepc`, with no trap having occurred at all.
    ///
    /// All-zero means "nothing pending", which is what a zero-initialized
    /// `Vcpu` must decode to.
    pub pending_resume: PendingResume,
    /// One-way "this vCPU cannot be resumed" flag.
    ///
    /// spec `sbi_covh_run_tvm_vcpu` §10.17: when the run returns a non-zero
    /// `sbiret.value`, "attempts to call `sbi_covh_run_tvm_vcpu()` with the
    /// same tvm_vcpu_id will fail". We set this from the TSM's own record
    /// of the last exit whenever we detect an unrecoverable condition:
    /// the exit info cannot be delivered to the host, or the pending-resume
    /// record was corrupted. False by default (a fresh vCPU is runnable),
    /// so the all-zero bit pattern of the parent `TVM` instance decodes
    /// correctly.
    pub terminated: AtomicBool,
}

impl Vcpu {
    // -----------------------------------------------------------------------
    // Lifecycle: new / create / power_on / activate / deactivate / reset
    // -----------------------------------------------------------------------

    /// Create a new vCPU with the given IDs.
    ///
    /// The vCPU starts in `Created` state. Each vCPU is bound to one hart,
    /// so `pcpu_id` equals `vcpu_id`.
    pub fn new(vcpu_id: usize, pcpu_id: usize) -> Self {
        Vcpu {
            regs: VmCpuRegisters::default(),
            status: AtomicU8::new(VcpuStatus::Created as u8),
            first_run: AtomicBool::new(false),
            vcpu_id,
            pcpu_id,
            imsic_gpa: 0,
            imsic_hw_addr: 0,
            pending_resume: PendingResume::NONE,
            terminated: AtomicBool::new(false),
        }
    }

    /// Initialize this vCPU slot for use by a TVM.
    ///
    /// Sets vcpu_id, marks pcpu as unbound (MAX_HARTS), and transitions
    /// status to Created. Called from `Tvm::create_vcpu` during CreateTvmVcpu.
    pub fn create(&mut self, vcpu_id: usize) {
        self.vcpu_id = vcpu_id;
        self.pcpu_id = config::MAX_HARTS;
        self.set_status(VcpuStatus::Created);
    }

    /// Reset all vCPU state to zero/default. Used when destroying a TVM.
    pub fn reset(&mut self) {
        self.regs = VmCpuRegisters::default();
        self.status
            .store(VcpuStatus::PoweredOff as u8, Ordering::Relaxed);
        self.first_run.store(false, Ordering::Relaxed);
        self.vcpu_id = 0;
        self.pcpu_id = 0;
        self.imsic_gpa = 0;
        self.imsic_hw_addr = 0;
        // Must not survive teardown: a stale record would let the next TVM's
        // first resume consume host input recorded for the previous one.
        self.pending_resume = PendingResume::NONE;
        // The termination flag is scoped to one TVM's lifetime, so a fresh
        // vCPU in a re-created TVM must not inherit an earlier flag.
        self.terminated.store(false, Ordering::Relaxed);
    }

    /// One-way "cannot be resumed" latch. spec `sbi_covh_run_tvm_vcpu` §10.17:
    /// after a non-zero sbiret.value, subsequent runs of the same vcpu_id
    /// must fail. Setting is cheap and idempotent.
    pub fn mark_terminated(&self) {
        self.terminated.store(true, Ordering::Release);
    }

    /// Whether this vCPU has been latched as unrecoverable.
    pub fn is_terminated(&self) -> bool {
        self.terminated.load(Ordering::Acquire)
    }

    /// Power on this vCPU with the given entry point and opaque argument.
    ///
    /// Atomically transitions Created/PoweredOff → Runnable, initializes
    /// boot registers (sepc, a0=vcpu_id, a1=opaque, sstatus, scounteren),
    /// and sets `first_run` flag. Used by both FinalizeTvm (boot vCPU) and
    /// Guest HSM HartStart (secondary vCPUs).
    ///
    /// Returns `Err(AlreadyPowered)` if the vCPU is not Created or PoweredOff.
    #[inline]
    pub fn power_on(&mut self, sepc: u64, opaque: u64) -> Result<(), VcpuError> {
        // Step 1: CAS to claim the vCPU (Acquire to prevent reordering)
        let result = self
            .status
            .fetch_update(Ordering::Acquire, Ordering::Acquire, |current| {
                if current == VcpuStatus::PoweredOff as u8 || current == VcpuStatus::Created as u8 {
                    // Temporarily mark as Created to prevent races
                    Some(VcpuStatus::Created as u8)
                } else {
                    None
                }
            });
        if result.is_err() {
            return Err(VcpuError::AlreadyPowered);
        }

        // Step 2: Initialize registers while status is Created (not yet Runnable)
        self.regs = VmCpuRegisters::default();
        self.regs.guest_regs.sepc = sepc;
        self.regs
            .guest_regs
            .gprs
            .set_reg(GprIndex::A0, self.vcpu_id as u64);
        self.regs.guest_regs.gprs.set_reg(GprIndex::A1, opaque);
        self.regs.guest_regs.sstatus = GUEST_INIT_SSTATUS;
        self.regs.guest_regs.scounteren = GUEST_INIT_SCOUNTEREN;
        self.first_run.store(true, Ordering::Relaxed);

        // Step 3: Release-store Runnable so activate() on another hart sees all writes above
        self.status
            .store(VcpuStatus::Runnable as u8, Ordering::Release);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Execution: activate / deactivate
    // -----------------------------------------------------------------------

    /// Activate this vCPU for execution: CAS Runnable → Running and return
    /// the `first_run` flag. This only performs the status transition and
    /// does not enter the guest.
    ///
    /// Returns `Ok(true)` if this is the first run after power_on (the
    /// caller should skip host resume state), `Ok(false)` for a normal
    /// resume.  Returns `Err(NotRunnable)` if the CAS fails.
    ///
    /// Full execution chain (driven by the ecall layer):
    /// `activate()` → (first-run: `Tvm::setup_vcpu_imsic` / resume: apply
    /// shmem resume state) → `guest_execution_loop` → `deactivate()`.
    /// The caller must eventually call `deactivate()`.
    #[inline]
    pub fn activate(&self) -> Result<bool, VcpuError> {
        // Acquire: ensure we see all writes from power_on (regs, first_run)
        if self
            .status
            .compare_exchange(
                VcpuStatus::Runnable as u8,
                VcpuStatus::Running as u8,
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .is_err()
        {
            return Err(VcpuError::NotRunnable);
        }

        let first = self.first_run.load(Ordering::Relaxed);
        if first {
            self.first_run.store(false, Ordering::Relaxed);
        }
        Ok(first)
    }

    /// Deactivate this vCPU after execution: CAS Running → Runnable.
    ///
    /// Uses a defensive CAS so that if HartStop already set PoweredOff,
    /// the status is left unchanged.
    #[inline]
    pub fn deactivate(&self) {
        // Release: ensure all register writes during guest execution are
        // visible before another hart sees Runnable status.
        let _ = self.status.compare_exchange(
            VcpuStatus::Running as u8,
            VcpuStatus::Runnable as u8,
            Ordering::Release,
            Ordering::Relaxed,
        );
    }

    // -----------------------------------------------------------------------
    // Status
    // -----------------------------------------------------------------------

    #[inline(always)]
    pub fn get_status(&self) -> VcpuStatus {
        VcpuStatus::from_u8(self.status.load(Ordering::Relaxed))
    }

    #[inline(always)]
    pub fn set_status(&self, s: VcpuStatus) {
        self.status.store(s as u8, Ordering::Relaxed);
    }

    // -----------------------------------------------------------------------
    // GPR accessors
    // -----------------------------------------------------------------------

    #[inline(always)]
    pub fn set_gpr(&mut self, gpr: GprIndex, value: u64) {
        self.regs.guest_regs.gprs.set_reg(gpr, value);
    }
}

impl Default for Vcpu {
    fn default() -> Self {
        Vcpu {
            regs: VmCpuRegisters::default(),
            status: AtomicU8::new(VcpuStatus::PoweredOff as u8),
            first_run: AtomicBool::new(false),
            vcpu_id: 0,
            pcpu_id: 0,
            imsic_gpa: 0,
            imsic_hw_addr: 0,
            pending_resume: PendingResume::NONE,
            terminated: AtomicBool::new(false),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
