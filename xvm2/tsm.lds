/* SPDX-FileCopyrightText: 2026 DAMO Inc.
 * SPDX-FileCopyrightText: 2023 Rivos Inc.
 *
 * SPDX-License-Identifier: Apache-2.0
 */

/*
 * Linker script for the CoVE TSM firmware.
 *
 * Target: bare-metal riscv64gc-unknown-none-elf. The QEMU guest-loader
 * drops the flat image at physical address 0xA0200000 and transfers
 * control to the ELF entry point, so the boot code must be the very
 * first thing in the image.
 */

OUTPUT_ARCH( "riscv" )

ENTRY( _start )

/*
 * One combined read/write/execute window covering the entire firmware:
 * code, read-only data, writable data, BSS and the two boot stacks.
 */
MEMORY
{
    ram  (wxa) : ORIGIN = 0xA0200000, LENGTH = 128M
}

/*
 * Segment layout used for loading: executable/read-only content, then
 * initialized writable data, then the zero-fill region.
 */
PHDRS
{
    text PT_LOAD;
    data PT_LOAD;
    bss PT_LOAD;
}

SECTIONS
{
    /* Bare-metal images carry no unwind tables. */
    /DISCARD/ : {
        *(.eh_frame)
    }

    /* Boot code: the loader jumps to the front of the image. */
    .start : {
        *(.text.start)
    } >ram AT>ram :text

    /* Startup code in front, the rest of the executable code behind it. */
    .text : {
        *(.text.init) *(.text .text.*)
    } >ram AT>ram :text

    /* Exception table consumed by the trap handler's fault fixup path:
     * quad-aligned entries collected from every faultable instruction. */
    .extable : {
        . = ALIGN(8);
        PROVIDE(__exception_table_begin = .);
        KEEP(*(.extable))
        PROVIDE(__exception_table_end = .);
    } >ram AT>ram :text

    /* Constants and string literals. */
    .rodata : {
        *(.rodata .rodata.*)
    } >ram AT>ram :text

    /* Writable data, page-aligned so the region can be covered with
     * page-granular hardware attributes. __global_pointer$ points 0x800
     * bytes into the data, which places the small-data areas on both
     * sides of gp inside its 2 KiB relative addressing window. */
    .data : {
        . = ALIGN(4096);
        *(.data .data.*)

        . = ALIGN(8);
        PROVIDE(__global_pointer$ = . + 0x800);
        *(.sdata .sdata.*)
    } >ram AT>ram :data

    /* Zero-initialized data. The bounds are consumed by the startup
     * code, which clears the region word by word. */
    .bss : {
        . = ALIGN(8);
        PROVIDE(__bss_begin = .);
        *(.sbss .sbss.*) *(.bss .bss.*)
        . = ALIGN(8);
        PROVIDE(__bss_end = .);
    } >ram AT>ram :bss

    /*
     * Boot stacks, laid out directly behind BSS on a fresh page:
     *
     *   __boot_stack_begin .. __boot_stack_end
     *       boot-hart stack (512 KiB; the TSM runs single-hart)
     *   __overflow_stack_begin .. __overflow_stack_end
     *       emergency stack for the stack-overflow trap handler (8 KiB)
     */
    PROVIDE(__boot_stack_begin = ALIGN(__bss_end, 4096));
    PROVIDE(__boot_stack_end = __boot_stack_begin + 0x80000);
    PROVIDE(__overflow_stack_begin = __boot_stack_end);
    PROVIDE(__overflow_stack_end = __overflow_stack_begin + 0x2000);
}
