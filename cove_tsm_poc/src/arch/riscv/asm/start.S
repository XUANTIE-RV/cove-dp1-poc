// Copyright (c) 2026 by DAMO Inc.
// Copyright (c) 2021 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSES/Apache-2.0.txt for details.
// SPDX-License-Identifier: Apache-2.0

/*
 * TSM boot entry point.
 *
 * This file defines the single hardware entry point of the TSM image:
 *
 *   _start - reset vector for the boot hart. Zero-fills the BSS region,
 *            installs the boot stack and transfers control to the Rust
 *            runtime. The TSM is single-hart: any hart arriving with a
 *            non-zero hart id is parked in a wfi loop as a safety net on
 *            multi-hart platforms.
 *
 * The path executes with address translation disabled and supervisor
 * interrupts masked until the Rust side takes over.
 */

.option norvc

.section .text.start

/* Boot-hart entry: the RDSM transfers control here exactly once. */
.global _start
_start:

.option push
.option norelax
    la gp, __global_pointer$
.option pop
    csrw sstatus, zero
    csrw sie, zero

    /*
     * Single-hart safety net: the TSM only supports hart 0. Should the
     * platform ever release another hart into this entry point, park it
     * in the idle loop instead of corrupting the boot-hart stack.
     */
    bnez a0, .L_idle_loop

    /*
     * Zero-fill the BSS region. The bounds __bss_begin/__bss_end are
     * emitted by the tsm.lds linker script and are 8-byte aligned, so a
     * simple doubleword store loop is sufficient.
     */
    la   a3, __bss_begin
    la   a4, __bss_end
.L_bss_next:
    sd   zero, (a3)
    addi a3, a3, 8
    blt  a3, a4, .L_bss_next

    /* Install the boot-hart stack; the top symbol comes from tsm.lds. */
    la   sp, __boot_stack_end
    call _primary_init
    /* _primary_init hands the SATP image back in a0; activate it. */
    csrw satp, a0
    /* Flush stale translations once paging is live. */
    //sfence.vma
    /* Move onto the VA-mapped hypervisor stack. */
    //li  sp, {HYP_STACK_TOP}
    call _primary_main

.L_idle_loop:
    wfi
    j    .L_idle_loop
