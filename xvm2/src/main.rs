// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2026 DAMO Inc.

//! CoVE TSM entry point — boots the hypervisor and installs trap handlers.
//!
//! This binary target orchestrates startup across workspace crates (`trap`,
//! `tvm`, `ecall`, `interrupt`) and provides the panic handler, allocator
//! stub, and CSR initialization sequence.

#![no_main]
#![no_std]
#![feature(
    allocator_api,
    alloc_error_handler,
    if_let_guard,
    slice_ptr_get,
    let_chains,
    negative_impls
)]
use core::alloc::{GlobalAlloc, Layout};
extern crate alloc;

mod arch;

use riscv_regs::{hedeleg, hideleg, hie, scounteren};
use riscv_regs::{Exception, Interrupt, LocalRegisterCopy, Writeable, CSR};
use sbi_rt::{ecall_send, SbiMessage};
use trap::backtrace::capture_backtrace;
use utils::abort::abort;
use utils::print::*;
use utils::sbi_console::PutCharConsoleV01;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    /// Number of internal panic-handling stack frames to skip when
    /// printing the backtrace (panic_handler → capture_backtrace → read_fp).
    const PANIC_SKIP_FRAMES: usize = 2;

    println!("panic : {:?}", info);
    if let Some(location) = info.location() {
        println!(
            "panic occurred in file '{}' at line {}",
            location.file(),
            location.line()
        );
    }

    println!("panic backtrace:");
    if let Some(bt) = capture_backtrace() {
        bt.skip(PANIC_SKIP_FRAMES).for_each(|frame| {
            print!("{}", frame);
        });
    }

    abort()
}

/// Stub global allocator — no heap allocations are performed at runtime.
struct StubAlloc;

unsafe impl GlobalAlloc for StubAlloc {
    unsafe fn alloc(&self, _layout: Layout) -> *mut u8 {
        core::ptr::null_mut()
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static GENERAL_ALLOCATOR: StubAlloc = StubAlloc;

/// Aborts if the system hits an allocation error.
#[alloc_error_handler]
pub fn alloc_error(_layout: Layout) -> ! {
    abort()
}

/// Initialize (H)S-level CSRs to a reasonable state.
///
/// Configures interrupt routing, trap delegation and counter access
/// for the hypervisor and its guests before any vCPU is launched.
pub fn setup_csrs() {
    // Clear and disable any interrupts.
    CSR.sie.set(0);
    CSR.sip.set(0);
    // Turn FP and vector units off.
    CSR.sstatus.set(0);

    // VCoVE: Set hstateen0 register to make extensions legal.
    // Bit mask enabling the extension state (ENVCFG, CSR indirection,
    // AIA/IMSIC and the `stateen` CSRs themselves) for the guest; without
    // it those accesses would trap.
    const HSTATEEN0_EXTENSION_ENABLE: u64 = 0xDC40000000000000;
    CSR.hstateen0.set(HSTATEEN0_EXTENSION_ENABLE);

    // -- Synchronous exception delegation to VS-mode --
    // These are the exceptions the guest OS should handle itself;
    // everything else traps into the hypervisor.
    let mut hedeleg = LocalRegisterCopy::<u64, hedeleg::Register>::new(0);
    hedeleg.modify(Exception::InstructionMisaligned.to_hedeleg_field().unwrap());
    hedeleg.modify(Exception::IllegalInstruction.to_hedeleg_field().unwrap());
    hedeleg.modify(Exception::Breakpoint.to_hedeleg_field().unwrap());
    hedeleg.modify(Exception::LoadMisaligned.to_hedeleg_field().unwrap());
    hedeleg.modify(Exception::StoreMisaligned.to_hedeleg_field().unwrap());
    hedeleg.modify(Exception::UserEnvCall.to_hedeleg_field().unwrap());
    hedeleg.modify(Exception::InstructionPageFault.to_hedeleg_field().unwrap());
    hedeleg.modify(Exception::LoadPageFault.to_hedeleg_field().unwrap());
    hedeleg.modify(Exception::StorePageFault.to_hedeleg_field().unwrap());
    CSR.hedeleg.set(hedeleg.get());

    // -- Interrupt delegation to VS-mode --
    // Virtual supervisor software/timer/external interrupts are
    // delivered directly to the guest without hypervisor intervention.
    let mut hideleg = LocalRegisterCopy::<u64, hideleg::Register>::new(0);
    hideleg.modify(Interrupt::VirtualSupervisorSoft.to_hideleg_field().unwrap());
    hideleg.modify(
        Interrupt::VirtualSupervisorTimer
            .to_hideleg_field()
            .unwrap(),
    );
    hideleg.modify(
        Interrupt::VirtualSupervisorExternal
            .to_hideleg_field()
            .unwrap(),
    );
    CSR.hideleg.set(hideleg.get());

    // -- Hypervisor interrupt enable --
    // Enable the VS-level interrupt sources at the H-level so they
    // actually get delivered when delegated.
    let mut hie = LocalRegisterCopy::<u64, hie::Register>::new(0);
    hie.modify(Interrupt::VirtualSupervisorSoft.to_hie_field().unwrap());
    hie.modify(Interrupt::VirtualSupervisorTimer.to_hie_field().unwrap());
    hie.modify(Interrupt::VirtualSupervisorExternal.to_hie_field().unwrap());
    CSR.hie.set(hie.get());

    // Enable access to cycle, time, and instret counters for the guest.
    CSR.hcounteren.set(0x7);

    // Make the basic counters available to any of our U-mode tasks.
    let mut scounteren = LocalRegisterCopy::<u64, scounteren::Register>::new(0);
    scounteren.modify(scounteren::cycle.val(1));
    scounteren.modify(scounteren::time.val(1));
    scounteren.modify(scounteren::instret.val(1));
    CSR.scounteren.set(scounteren.get());

    trap::install_init_trap_handler();
}

#[repr(C)]
struct CpuParams {
    satp: u64,
}

/// Bootstrap CPU initialization, executed under 1:1 physical mapping.
///
/// Stores the hart ID, configures CSRs, and brings up the SBI console.
/// Returns the SATP value for the hypervisor page table (0 = identity).
#[no_mangle]
extern "C" fn _primary_init(hart_id: u64, _fdt_addr: u64) -> CpuParams {
    // Store hart_id into tp register for per-hart state indexing.
    unsafe {
        tvm::store_hart_id(hart_id);
    }
    setup_csrs();
    PutCharConsoleV01::set_as_console();
    // Console is now available: report that CSR setup and console init completed.
    println!(
        "primary hart {} init done: CSRs configured, console ready",
        hart_id
    );
    println!("TEST: TSM Boot {}", hart_id);
    println!("PASS: TSM Boot {}", hart_id);
    CpuParams { satp: 0 }
}

/// Steady-state entry point after the hypervisor page table is active.
///
/// Installs the final trap handler and performs a TEE Return to transfer
/// control to the RDSM (Root Domain Security Monitor).
#[no_mangle]
extern "C" fn _primary_main() {
    trap::install_trap_handler();
    println!("primary hart: trap handler installed");

    println!("initialize done, ECALL to RDSM by TEERET");
    let teeret_message = SbiMessage::TeeRet([0, 0]);
    unsafe {
        ecall_send::<()>(&teeret_message).unwrap();
    }
    panic!("Fail to return to RDSM!");
}
