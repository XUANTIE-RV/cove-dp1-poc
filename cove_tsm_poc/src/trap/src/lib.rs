// SPDX-FileCopyrightText: 2026 DAMO Inc.
// SPDX-FileCopyrightText: 2023 Rivos Inc.
//
// SPDX-License-Identifier: Apache-2.0

#![no_std]

extern crate alloc;

#[cfg(all(target_arch = "riscv64", target_os = "none"))]
pub mod backtrace;
pub mod hyp_layout;

#[cfg(all(target_arch = "riscv64", target_os = "none"))]
mod extable;
#[cfg(all(target_arch = "riscv64", target_os = "none"))]
mod frame;

#[cfg(all(target_arch = "riscv64", target_os = "none"))]
mod trap_impl {
    use core::arch::global_asm;
    use core::mem::size_of;

    use memoffset::offset_of;
    use riscv_regs::{sie, Exception, GprIndex, Readable, RiscvCsrInterface, Trap, Writeable, CSR};
    use sbi_rt::*;
    use utils::print::*;

    use super::extable::pc_in_extable;
    use super::frame::TrapFrame;
    use super::hyp_layout::HYP_STACK_BOTTOM;
    use ecall::{handle_cove_host_msg, handle_cove_interrupt_msg};
    use interrupt::handle_interrupt;

    // Name matches the `overflow_stack_lock` extern reference in trap.S,
    // so the symbol must keep its lowercase form.
    #[allow(non_upper_case_globals)]
    #[no_mangle]
    static overflow_stack_lock: u64 = 0;

    extern "C" {
        fn _trap_entry();
        fn _trap_init_entry();
    }

    const fn gpr_offset(index: GprIndex) -> usize {
        offset_of!(TrapFrame, gprs) + (index as usize) * size_of::<u64>()
    }

    global_asm!(
        include_str!("../../arch/riscv/asm/trap.S"),
        tf_size = const size_of::<TrapFrame>(),
        tf_ra = const gpr_offset(GprIndex::RA),
        tf_gp = const gpr_offset(GprIndex::GP),
        tf_tp = const gpr_offset(GprIndex::TP),
        tf_s0 = const gpr_offset(GprIndex::S0),
        tf_s1 = const gpr_offset(GprIndex::S1),
        tf_a0 = const gpr_offset(GprIndex::A0),
        tf_a1 = const gpr_offset(GprIndex::A1),
        tf_a2 = const gpr_offset(GprIndex::A2),
        tf_a3 = const gpr_offset(GprIndex::A3),
        tf_a4 = const gpr_offset(GprIndex::A4),
        tf_a5 = const gpr_offset(GprIndex::A5),
        tf_a6 = const gpr_offset(GprIndex::A6),
        tf_a7 = const gpr_offset(GprIndex::A7),
        tf_s2 = const gpr_offset(GprIndex::S2),
        tf_s3 = const gpr_offset(GprIndex::S3),
        tf_s4 = const gpr_offset(GprIndex::S4),
        tf_s5 = const gpr_offset(GprIndex::S5),
        tf_s6 = const gpr_offset(GprIndex::S6),
        tf_s7 = const gpr_offset(GprIndex::S7),
        tf_s8 = const gpr_offset(GprIndex::S8),
        tf_s9 = const gpr_offset(GprIndex::S9),
        tf_s10 = const gpr_offset(GprIndex::S10),
        tf_s11 = const gpr_offset(GprIndex::S11),
        tf_t0 = const gpr_offset(GprIndex::T0),
        tf_t1 = const gpr_offset(GprIndex::T1),
        tf_t2 = const gpr_offset(GprIndex::T2),
        tf_t3 = const gpr_offset(GprIndex::T3),
        tf_t4 = const gpr_offset(GprIndex::T4),
        tf_t5 = const gpr_offset(GprIndex::T5),
        tf_t6 = const gpr_offset(GprIndex::T6),
        tf_sp = const gpr_offset(GprIndex::SP),
        tf_sstatus = const offset_of!(TrapFrame, sstatus),
        tf_sepc = const offset_of!(TrapFrame, sepc),
        hyp_stack_bottom = const HYP_STACK_BOTTOM,
    );

    #[no_mangle]
    extern "C" fn handle_stack_overflow(tf_ptr: *mut TrapFrame) {
        let tf = unsafe { tf_ptr.as_mut().unwrap() };
        println!("Stack overflow (please note: T1 register is clobbered below)");
        println!("{}", tf);
        panic!("Stack overflow!");
    }

    #[no_mangle]
    extern "C" fn handle_trap(tf_ptr: *mut TrapFrame) {
        let tf = unsafe { tf_ptr.as_mut().unwrap() };
        let scause = CSR.scause.get();
        let stval = CSR.stval.get();

        if let Ok(t) = Trap::from_scause(scause) {
            match t {
                Trap::Interrupt(i) => {
                    if handle_interrupt(i) {
                        return;
                    }
                }
                Trap::Exception(e) => match e {
                    Exception::SupervisorEnvCall => {
                        let (mut ret_error, mut ret_value): (i64, i64) = (0, 0);
                        match SbiMessage::from_regs(tf.gprs.a_regs()) {
                            Ok(sbi_msg) => match sbi_msg {
                                SbiMessage::CoveHost(host_func) => {
                                    let (e, v) = handle_cove_host_msg(host_func);
                                    ret_error = e;
                                    ret_value = v;
                                }
                                SbiMessage::CoveInterrupt(int_func) => {
                                    let (e, v) = handle_cove_interrupt_msg(int_func);
                                    ret_error = e;
                                    ret_value = v;
                                }
                                _ => {
                                    println!(
                                        "Unhandled SBI ecall: a7=0x{:x} a6=0x{:x}",
                                        tf.gprs.reg(GprIndex::A7),
                                        tf.gprs.reg(GprIndex::A6)
                                    );
                                }
                            },
                            Err(e) => {
                                println!("Failed to parse SBI message: {:?}", e);
                                println!("a0-a7: {:x?}", tf.gprs.a_regs());
                            }
                        }
                        // TeeRet: return to RDSM/host. In normal operation this
                        // ecall NEVER returns (RDSM switches domain). If it does
                        // return, something is seriously wrong — loop forever.
                        unsafe {
                            let mut attempts: u32 = 0;
                            loop {
                                let a0: i64;
                                let a1: i64;
                                core::arch::asm!("ecall",
                                    inlateout("a0") ret_error => a0,
                                    inlateout("a1") ret_value => a1,
                                    in("a2") 0i64, in("a3") 0i64,
                                    in("a4") 0i64, in("a5") 0i64,
                                    in("a6") 0i64,
                                    in("a7") 0x434F5650u64, // EXT_TEE_RET
                                    options(nostack),
                                );
                                // If we reach here, ecall returned to TSM (abnormal!)
                                attempts += 1;
                                if attempts <= 3 {
                                    println!(
                                        "BUG: TeeRet returned to TSM! a0={} a1=0x{:x} attempt={}",
                                        a0, a1, attempts
                                    );
                                }
                                // Brief spin before retry to avoid flooding
                                for _ in 0..1000 {
                                    core::hint::spin_loop();
                                }
                            }
                        }
                    }
                    _ => {
                        if pc_in_extable(tf.sepc) {
                            tf.sepc = tf.gprs.reg(GprIndex::T0);
                            tf.gprs.set_reg(GprIndex::T1, scause);
                            return;
                        }
                    }
                },
            };
            print!("Unexpected trap: {}, ", t);
        } else {
            print!("Unexpected trap: <not decoded>, ");
        }

        println!("SCAUSE: 0x{:08x}, STVAL: 0x{:08x}", scause, stval);
        println!("{}", tf);

        panic!("Unexpected trap");
    }

    /// Installs init-time handler for HS-level traps.
    pub fn install_init_trap_handler() {
        CSR.stvec
            .set((_trap_init_entry as usize).try_into().unwrap());
        CSR.sie.read_and_set_bits(1 << sie::sext.shift);
    }

    /// Installs a handler for HS-level traps.
    pub fn install_trap_handler() {
        CSR.stvec.set((_trap_entry as usize).try_into().unwrap());
        CSR.sie.read_and_set_bits(1 << sie::sext.shift);
    }
}

#[cfg(all(target_arch = "riscv64", target_os = "none"))]
pub use trap_impl::{install_init_trap_handler, install_trap_handler};
