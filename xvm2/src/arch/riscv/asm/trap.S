// Copyright (c) 2026 by DAMO Inc.
// Copyright (c) 2022 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSES/Apache-2.0.txt for details.
// SPDX-License-Identifier: Apache-2.0

/*
 * Supervisor trap entry / exit paths for the TSM.
 *
 * Two vectors are exported:
 *
 *   _trap_init_entry - installed while the hypervisor address space is
 *                      still being brought up. It cannot rely on the
 *                      stack-bound checks, so it jumps straight to the
 *                      common frame-save path.
 *   _trap_entry      - the steady-state vector. It first probes the
 *                      stack pointer against the hypervisor stack floor;
 *                      an overflowing hart is diverted onto a shared
 *                      emergency stack (guarded by a spinlock) where it
 *                      panics without ever touching the corrupted stack.
 *
 * A full trap frame (all GPRs plus sstatus/sepc and the pre-trap SP) is
 * materialised on the stack and handed to the Rust handlers by pointer.
 * The frame layout offsets arrive as tf_* template parameters injected
 * through global_asm! from the trap crate.
 */

.attribute arch, "rv64gcv"
.section .text.init

/*
 * Push a complete trap frame onto the current stack.
 *
 * On entry sscratch must hold the stack pointer value that should be
 * recorded inside the frame; sscratch is consumed (zeroed) by the
 * expansion. t0/t1/t2 are captured before being reused as scratch for
 * the CSR reads at the tail.
 */
.macro store_trap_frame
    addi sp, sp, -({tf_size})
    /* ra, gp, tp: linkage and thread-local base */
    sd   ra, ({tf_ra})(sp)
    sd   gp, ({tf_gp})(sp)
    sd   tp, ({tf_tp})(sp)
    /* s0-s1: callee-saved frame pointer and scratch */
    sd   s0, ({tf_s0})(sp)
    sd   s1, ({tf_s1})(sp)
    /* a0-a7: argument registers */
    sd   a0, ({tf_a0})(sp)
    sd   a1, ({tf_a1})(sp)
    sd   a2, ({tf_a2})(sp)
    sd   a3, ({tf_a3})(sp)
    sd   a4, ({tf_a4})(sp)
    sd   a5, ({tf_a5})(sp)
    sd   a6, ({tf_a6})(sp)
    sd   a7, ({tf_a7})(sp)
    /* s2-s11: remaining callee-saved registers */
    sd   s2, ({tf_s2})(sp)
    sd   s3, ({tf_s3})(sp)
    sd   s4, ({tf_s4})(sp)
    sd   s5, ({tf_s5})(sp)
    sd   s6, ({tf_s6})(sp)
    sd   s7, ({tf_s7})(sp)
    sd   s8, ({tf_s8})(sp)
    sd   s9, ({tf_s9})(sp)
    sd   s10, ({tf_s10})(sp)
    sd   s11, ({tf_s11})(sp)
    /* t0-t6: temporaries (caller-saved) */
    sd   t0, ({tf_t0})(sp)
    sd   t1, ({tf_t1})(sp)
    sd   t2, ({tf_t2})(sp)
    sd   t3, ({tf_t3})(sp)
    sd   t4, ({tf_t4})(sp)
    sd   t5, ({tf_t5})(sp)
    sd   t6, ({tf_t6})(sp)
    /* CSR context: sp (from sscratch), sstatus, sepc */
    csrr t0, sscratch
    sd   t0, ({tf_sp})(sp)
    csrw sscratch, zero
    csrr t1, sstatus
    sd   t1, ({tf_sstatus})(sp)
    csrr t2, sepc
    sd   t2, ({tf_sepc})(sp)
.endm

/*
 * Early trap vector. Hypervisor mappings (and thus the stack bounds)
 * are not established yet, so the overflow probe is bypassed.
 */
.global _trap_init_entry
.balign 4
_trap_init_entry:
    /* No overflow probing this early in boot. */
    j .L_normal_path

/* Steady-state trap vector. */
.global _trap_entry
.balign 4
_trap_entry:
    /* Park t0 in sscratch so it can serve as scratch for the probe. */
    csrw sscratch, t0
    /* Divert to the emergency path if SP sank below the stack floor. */
    li t0, {hyp_stack_bottom}
    blt sp, t0, _stack_overflow
    /* Probe passed: recover t0 from sscratch. */
    csrr t0, sscratch

.L_normal_path:
    /* Record the pre-trap SP, then build the frame on this stack. */
    csrw sscratch, sp
    store_trap_frame

    /* Hand the frame pointer to the Rust dispatcher. */
    move a0, sp
    call handle_trap

    /* Unwind the frame and resume the interrupted context. */
    /* CSR context: sstatus, sepc */
    ld   t0, ({tf_sstatus})(sp)
    csrw sstatus, t0
    ld   t1, ({tf_sepc})(sp)
    csrw sepc, t1
    /* ra, gp, tp: linkage and thread-local base */
    ld   ra, ({tf_ra})(sp)
    ld   gp, ({tf_gp})(sp)
    ld   tp, ({tf_tp})(sp)
    /* s0-s1: callee-saved frame pointer and scratch */
    ld   s0, ({tf_s0})(sp)
    ld   s1, ({tf_s1})(sp)
    /* a0-a7: argument registers */
    ld   a0, ({tf_a0})(sp)
    ld   a1, ({tf_a1})(sp)
    ld   a2, ({tf_a2})(sp)
    ld   a3, ({tf_a3})(sp)
    ld   a4, ({tf_a4})(sp)
    ld   a5, ({tf_a5})(sp)
    ld   a6, ({tf_a6})(sp)
    ld   a7, ({tf_a7})(sp)
    /* s2-s11: remaining callee-saved registers */
    ld   s2, ({tf_s2})(sp)
    ld   s3, ({tf_s3})(sp)
    ld   s4, ({tf_s4})(sp)
    ld   s5, ({tf_s5})(sp)
    ld   s6, ({tf_s6})(sp)
    ld   s7, ({tf_s7})(sp)
    ld   s8, ({tf_s8})(sp)
    ld   s9, ({tf_s9})(sp)
    ld   s10, ({tf_s10})(sp)
    ld   s11, ({tf_s11})(sp)
    /* t0-t6: temporaries (caller-saved) */
    ld   t0, ({tf_t0})(sp)
    ld   t1, ({tf_t1})(sp)
    ld   t2, ({tf_t2})(sp)
    ld   t3, ({tf_t3})(sp)
    ld   t4, ({tf_t4})(sp)
    ld   t5, ({tf_t5})(sp)
    ld   t6, ({tf_t6})(sp)
    /* sp: restored from the saved pre-trap value */
    ld   sp, ({tf_sp})(sp)
    sret

_stack_overflow:
    /* Serialise access to the single emergency stack with an AMO-based
     * spinlock, following the "spinlock with fences" pattern from the
     * RISC-V Memory Consistency Model specification (draft of Dec 1st
     * 2017, section 2.3.5).
     */
    la t1, overflow_stack_lock
    li t0, 1
.L_lock_retry:
    amoswap.w t0, t0, (t1)
    fence r, rw
    bnez t0, .L_lock_retry

    /* Recover t0, parked in sscratch at the top of _trap_entry. */
    csrr t0, sscratch
    /* NOTE: t1 is clobbered and will read back garbage from the frame. */

    csrw sscratch, sp
    /* Emergency stack top, provided by the tsm.lds linker script. */
    la sp, __overflow_stack_end
    store_trap_frame

    /* Report the overflow to Rust; this call never returns. */
    move a0, sp
    call handle_stack_overflow
.L_halt_forever:
    wfi
    j .L_halt_forever
