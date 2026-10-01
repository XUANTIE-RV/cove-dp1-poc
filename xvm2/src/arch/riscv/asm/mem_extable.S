// Copyright (c) 2026 by DAMO Inc.
// Copyright (c) 2022 by Rivos Inc.
// Licensed under the Apache License, Version 2.0, see LICENSES/Apache-2.0.txt for details.
// SPDX-License-Identifier: Apache-2.0

/*
 * Fault-tolerant memory copy primitives for guest and user memory.
 *
 * Guest accesses go through the H-extension HLV/HSV/HLVX instructions,
 * which perform the load/store under the two-stage translation
 * currently programmed in hgatp/vsatp. Any of these accesses may fault
 * (e.g. an unmapped or converted guest page), so every faultable
 * instruction is registered in the .extable section via the
 * add_extable macro below.
 *
 * Recovery contract shared with handle_trap:
 *   - t0 holds the recovery address to jump to when a registered
 *     instruction faults (the handler also deposits SCAUSE in t1);
 *   - t2 carries the running byte count, which _ret_from_copy moves
 *     into a0 so callers observe the number of bytes actually copied.
 *
 * The loops below copy one byte per iteration; they favour simplicity
 * and faultability over throughput.
 */

.attribute arch, "rv64gcvh"

/*
 * Register the instruction at label \lbl in the exception table: emit
 * its address as an 8-byte-aligned quad into the allocatable .extable
 * section, leaving the current section untouched.
 */
.macro add_extable lbl
.pushsection .extable, "a"
.balign      8
.quad        \lbl
.popsection
.endm

.section .text

/* Byte-wise copy into guest physical memory via hsv.b.
 *
 * Arguments:
 *   a0: destination guest physical address.
 *   a1: source pointer in hypervisor address space.
 *   a2: number of bytes to copy.
 *
 * Returns: number of bytes successfully copied before any fault.
 */
.global _copy_to_guest
_copy_to_guest:
    // Per the recovery contract, t0 points at the bail-out target for
    // handle_trap; SCAUSE will show up in t1.
    la    t0, _ret_from_copy
    // t2 counts copied bytes and becomes the return value.
    mv    t2, zero
1:
    beq   t2, a2, _ret_from_copy
    lb    t3, (a1)
2:
    hsv.b t3, (a0)
    add_extable 2b
    addi  a0, a0, 1
    addi  a1, a1, 1
    addi  t2, t2, 1
    j     1b

/* Byte-wise copy out of guest physical memory via hlv.b.
 *
 * Arguments:
 *   a0: destination pointer in hypervisor address space.
 *   a1: source guest physical address.
 *   a2: number of bytes to copy.
 *
 * Returns: number of bytes successfully copied before any fault.
 */
.global _copy_from_guest
_copy_from_guest:
    // Per the recovery contract, t0 points at the bail-out target for
    // handle_trap; SCAUSE will show up in t1.
    la    t0, _ret_from_copy
    // t2 counts copied bytes and becomes the return value.
    mv    t2, zero
1:
    beq   t2, a2, _ret_from_copy
2:
    hlv.b t3, (a1)
    add_extable 2b
    sb    t3, (a0)
    addi  a0, a0, 1
    addi  a1, a1, 1
    addi  t2, t2, 1
    j     1b

/* Fetch one guest instruction (2 or 4 bytes) with hlvx.hu.
 *
 * Arguments:
 *   a0: guest virtual address of the instruction, translated through
 *       the modes/tables currently active in hgatp and vsatp.
 *   a1: destination pointer to a u32 receiving the instruction bytes.
 *
 * Returns 0 on success, -1 if any halfword access faulted.
 */
.global _fetch_guest_instruction
_fetch_guest_instruction:
    // Per the recovery contract, t0 points at the bail-out target for
    // handle_trap; SCAUSE will show up in t1.
    la    t0, 4f
1:
    hlvx.hu t2, (a0)
    add_extable 1b
    sh    t2, (a1)
    addi  a0, a0, 2
    addi  a1, a1, 2
    // Compressed encodings have bits [1:0] != 'b11 and are complete
    // after a single halfword.
    li    t3, 3
    and   t2, t2, t3
    bne   t2, t3, 3f
    // 32-bit encoding: pull in the second halfword.
2:
    hlvx.hu t2, (a0)
    add_extable 2b
    sh    t2, (a1)
3:
    mv    a0, zero
    ret
4:
    // A registered access faulted: report -1.
    not   a0, zero
    ret

/* Byte-wise copy into user memory with a plain sb, fault-protected.
 *
 * Arguments:
 *   a0: destination user virtual address.
 *   a1: source pointer in hypervisor address space.
 *   a2: number of bytes to copy.
 *
 * Returns: number of bytes successfully copied before any fault.
 */
.global _copy_to_user
_copy_to_user:
   // Per the recovery contract, t0 points at the bail-out target for
   // handle_trap; SCAUSE will show up in t1.
   la    t0, _ret_from_copy
   // t2 counts copied bytes and becomes the return value.
   mv    t2, zero
1:
   beq   t2, a2, _ret_from_copy
   lb    t3, (a1)
2:
   sb t3, (a0)
   add_extable 2b
   addi  a0, a0, 1
   addi  a1, a1, 1
   addi  t2, t2, 1
   j     1b

/* Byte-wise copy out of user memory with a plain lb, fault-protected.
 *
 * Arguments:
 *   a0: destination pointer in hypervisor address space.
 *   a1: source user virtual address.
 *   a2: number of bytes to copy.
 *
 * Returns: number of bytes successfully copied before any fault.
 */
.global _copy_from_user
_copy_from_user:
   // Per the recovery contract, t0 points at the bail-out target for
   // handle_trap; SCAUSE will show up in t1.
   la    t0, _ret_from_copy
   // t2 counts copied bytes and becomes the return value.
   mv    t2, zero
1:
   beq   t2, a2, _ret_from_copy
2:
   lb t3, (a1)
   add_extable 2b
   sb    t3, (a0)
   addi  a0, a0, 1
   addi  a1, a1, 1
   addi  t2, t2, 1
   j     1b

.align 2
_ret_from_copy:
    mv    a0, t2
    ret
