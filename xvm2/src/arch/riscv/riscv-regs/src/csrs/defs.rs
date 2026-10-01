// SPDX-FileCopyrightText: 2026 DAMO Inc.
// SPDX-FileCopyrightText: 2023 Rivos Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Bitfield layouts for the supervisor, hypervisor, vector and AIA
//! CSRs consumed by the rest of the crate.

use tock_registers::register_bitfields;
use tock_registers::LocalRegisterCopy;

// ------------------------------------------------------------------
// Hypervisor (H-extension) registers
// ------------------------------------------------------------------

// Hypervisor status register.
register_bitfields![u64,
    pub hstatus [
        // Endianness selector for VS-mode.
        vsbe OFFSET(5) NUMBITS(1) [],
        // Set when the trap wrote a guest virtual address into stval.
        gva OFFSET(6) NUMBITS(1) [],
        // Virtualization mode that was active when the trap fired.
        spv OFFSET(7) NUMBITS(1) [],
        // Privilege level the virtual hart ran at before entering HS-mode.
        spvp OFFSET(8) NUMBITS(1) [
            User = 0,
            Supervisor = 1,
        ],
        // Permit hypervisor load/store instructions from U-mode.
        hu OFFSET(9) NUMBITS(1) [],
        // Guest external interrupt source routed to VS external interrupts.
        vgein OFFSET(12) NUMBITS(6) [],
        // Trap on SFENCE, SINVAL, or writes to vsatp.
        vtvm OFFSET(20) NUMBITS(1) [],
        // Trap when WFI exceeds its timeout.
        vtw OFFSET(21) NUMBITS(1) [],
        // Trap the SRET instruction.
        vtsr OFFSET(22) NUMBITS(1) [],
        // Effective base integer ISA width for VS-mode.
        vsxl OFFSET(32) NUMBITS(2) [
            Xlen32 = 1,
            Xlen64 = 2,
        ],
    ]
];

// Hypervisor exception delegation register.
register_bitfields![u64,
    pub hedeleg [
        instr_misaligned OFFSET(0) NUMBITS(1) [],
        instr_fault OFFSET(1) NUMBITS(1) [],
        illegal_instr OFFSET(2) NUMBITS(1) [],
        breakpoint OFFSET(3) NUMBITS(1) [],
        load_misaligned OFFSET(4) NUMBITS(1) [],
        load_fault OFFSET(5) NUMBITS(1) [],
        store_misaligned OFFSET(6) NUMBITS(1) [],
        store_fault OFFSET(7) NUMBITS(1) [],
        u_ecall OFFSET(8) NUMBITS(1) [],
        instr_page_fault OFFSET(12) NUMBITS(1) [],
        load_page_fault OFFSET(13) NUMBITS(1) [],
        store_page_fault OFFSET(15) NUMBITS(1) [],
    ]
];

// Hypervisor interrupt delegation register.
register_bitfields![u64,
    pub hideleg [
        vssoft OFFSET(2) NUMBITS(1) [],
        vstimer OFFSET(6) NUMBITS(1) [],
        vsext OFFSET(10) NUMBITS(1) [],
    ]
];

// Hypervisor interrupt enable register.
register_bitfields![u64,
    pub hie [
        vssoft OFFSET(2) NUMBITS(1) [],
        vstimer OFFSET(6) NUMBITS(1) [],
        vsext OFFSET(10) NUMBITS(1) [],
        sgext OFFSET(12) NUMBITS(1) [],
    ]
];

// Counter availability control for VS-mode.
register_bitfields![u64,
    pub hcounteren [
        cycle OFFSET(0) NUMBITS(1) [],
        time OFFSET(1) NUMBITS(1) [],
        instret OFFSET(2) NUMBITS(1) [],
        hpm OFFSET(3) NUMBITS(29) [],
    ]
];

// Hypervisor guest external interrupt enable.
register_bitfields![u64,
    pub hgeie [
        // How many bits are implemented is dictated by the platform GEILEN.
        interrupts OFFSET(1) NUMBITS(63) [],
    ]
];

// Hypervisor virtual interrupt control.
register_bitfields![u64,
    pub hvictl [
        iprio OFFSET(0) NUMBITS(8) [],
        ipriom OFFSET(8) NUMBITS(1) [],
        iid OFFSET(16) NUMBITS(12) [],
        vti OFFSET(30) NUMBITS(1) [],
    ]
];

// Faulting guest physical address.
register_bitfields![u64,
    pub htval [
        gpa_div4 OFFSET(0) NUMBITS(64) [],
    ]
];

pub trait HtvalHelpers {
    fn get_gpa(&self) -> u64;
}

impl HtvalHelpers for LocalRegisterCopy<u64, htval::Register> {
    fn get_gpa(&self) -> u64 {
        self.read(htval::gpa_div4) << 2
    }
}

// Hypervisor interrupt pending register.
register_bitfields![u64,
    pub hip [
        vssoft OFFSET(2) NUMBITS(1) [],
        vstimer OFFSET(6) NUMBITS(1) [],
        vsext OFFSET(10) NUMBITS(1) [],
        sgext OFFSET(12) NUMBITS(1) [],
    ]
];

// Hypervisor virtual interrupt pending.
register_bitfields![u64,
    pub hvip [
        vssoft OFFSET(2) NUMBITS(1) [],
        vstimer OFFSET(6) NUMBITS(1) [],
        vsext OFFSET(10) NUMBITS(1) [],
    ]
];

// Hypervisor trap instruction.
register_bitfields![u64,
    pub htinst [
        // Possibly a transformed encoding; refer to the privileged spec.
        instruction OFFSET(0) NUMBITS(64) [],
    ]
];

// Hypervisor guest external interrupt pending.
register_bitfields![u64,
    pub hgeip [
        // How many bits are implemented is dictated by the platform GEILEN.
        interrupts OFFSET(1) NUMBITS(63) [],
    ]
];

// Hypervisor environment configuration register.
register_bitfields![u64,
    pub henvcfg [
        // A fence targeting I/O also orders main memory.
        fiom OFFSET(0) NUMBITS(1) [],
        // Cache Block Invalidate instruction enable.
        cbie OFFSET(4) NUMBITS(2) [],
        // Cache Block Clean and Flush instruction enable.
        cbcfe OFFSET(6) NUMBITS(1) [],
        // Cache Block Zero instruction enable.
        cbze OFFSET(7) NUMBITS(1) [],
        // Hardware A/D bit updates while in VS-mode.
        adue OFFSET(61) NUMBITS(1) [],
        // Page-based memory types enable.
        pbmte OFFSET(62) NUMBITS(1) [],
        // Expose stimecmp to VS-mode.
        stce OFFSET(63) NUMBITS(1) [],
    ]
];

// Hypervisor (second-stage) address translation register.
register_bitfields![u64,
    pub hgatp [
        // Physical page number of the root translation table.
        ppn OFFSET(0) NUMBITS(44) [],
        // Virtual machine ID.
        vmid OFFSET(44) NUMBITS(14) [],
        // Translation mode.
        mode OFFSET(60) NUMBITS(4) [
            Bare = 0,
            Sv39x4 = 8,
            Sv48x4 = 9,
            Sv57x4 = 10,
        ],
    ]
];

// Hypervisor state-enable bits.
register_bitfields![u64,
    pub hstateen0 [
        // Expose counters to VS-mode.
        ctr OFFSET(54) NUMBITS(1) [],
        // Expose IMSIC functionality to VS-mode.
        aia_imsic OFFSET(58) NUMBITS(1) [],
        // Expose AIA functionality to VS-mode.
        aia OFFSET(59) NUMBITS(1) [],
        // Expose AIA indirect CSR selection to VS-mode.
        csrind OFFSET(60) NUMBITS(1) [],
        // Expose senvcfg to VS-mode.
        envcfg OFFSET(62) NUMBITS(1) [],
        // Expose sstateen0 to VS-mode.
        se0 OFFSET(63) NUMBITS(1) [],
    ]
];

// Hypervisor time offset register.
register_bitfields![u64,
    pub htimedelta [
        // Offset applied to 'time' reads issued from VS/VU modes.
        delta OFFSET(0) NUMBITS(64) [],
    ]
];

// ------------------------------------------------------------------
// Supervisor (S-mode) registers
// ------------------------------------------------------------------

// Supervisor status.
register_bitfields![u64,
    pub sstatus [
        // Global S-mode interrupt enable.
        sie OFFSET(1) NUMBITS(1) [],
        // Records whether supervisor interrupts were enabled before
        // the trap into S-mode.
        spie OFFSET(5) NUMBITS(1) [],
        // Big-endian enable for U-mode.
        ube OFFSET(6) NUMBITS(1) [],
        // Privilege level the hart ran at before entering S-mode.
        spp OFFSET(8) NUMBITS(1) [
            User = 0,
            Supervisor = 1,
        ],
        // State of the vector unit.
        vs OFFSET(9) NUMBITS(2) [
            Off = 0,
            Initial = 1,
            Clean = 2,
            Dirty = 3,
        ],
        // State of the floating-point unit.
        fs OFFSET(13) NUMBITS(2) [
            Off = 0,
            Initial = 1,
            Clean = 2,
            Dirty = 3,
        ],
        // State of additional U-mode extensions and their
        // associated context.
        xs OFFSET(15) NUMBITS(2) [
            AllOff = 0,
            SomeOn = 1,
            SomeClean = 2,
            SomeDirty = 3,
        ],
        // Supervisor User Memory - controls the privilege with which
        // S-mode loads and stores reach virtual memory.
        sum OFFSET(18) NUMBITS(1) [],
        // Make eXecutable Readable - controls the privilege with which
        // loads reach virtual memory.
        mxr OFFSET(19) NUMBITS(1) [],
        // Effective base integer ISA width for U-mode.
        uxl OFFSET(32) NUMBITS(2) [
            Xlen32 = 1,
            Xlen64 = 2,
        ],
        // Set when either FS or XS flags dirty state that would need
        // to be written back to memory on a context switch.
        sd OFFSET(63) NUMBITS(1) [],
    ]
];

// Supervisor interrupt enable register.
register_bitfields![u64,
    pub sie [
        ssoft OFFSET(1) NUMBITS(1) [],
        stimer OFFSET(5) NUMBITS(1) [],
        sext OFFSET(9) NUMBITS(1) [],
    ]
];

// Trap handler base address.
register_bitfields![u64,
    pub stvec [
        trap_addr OFFSET(2) NUMBITS(60) [],
        mode OFFSET(0) NUMBITS(2) [
            Direct = 0,
            Vectored = 1
        ]
    ]
];

pub trait StvecHelpers {
    fn get_trap_address(&self) -> u64;
}

impl StvecHelpers for LocalRegisterCopy<u64, stvec::Register> {
    fn get_trap_address(&self) -> u64 {
        self.read(stvec::trap_addr) << 2
    }
}

// Counter availability control for U-mode.
register_bitfields![u64,
    pub scounteren [
        cycle OFFSET(0) NUMBITS(1) [],
        time OFFSET(1) NUMBITS(1) [],
        instret OFFSET(2) NUMBITS(1) [],
        hpm OFFSET(3) NUMBITS(29) [],
    ]
];

// Scratch register reserved for supervisor use.
register_bitfields![u64,
    pub sscratch [
        val OFFSET(0) NUMBITS(64) []
    ]
];

// Address at which a trap was taken in HS-mode.
register_bitfields![u64,
    pub sepc [
        trap_addr OFFSET(0) NUMBITS(64) []
    ]
];

// Trap cause.
register_bitfields![u64,
    pub scause [
        is_interrupt OFFSET(63) NUMBITS(1) [],
        reason OFFSET(0) NUMBITS(63) []
    ],
    // The spec permits implementations to reuse the upper bits of the
    // interrupt/exception reason for their own purposes. Ordinary
    // decoding only looks at the "standard" values.
    pub(crate) reason [
        reserved OFFSET(5) NUMBITS(58) [],
        std OFFSET(0) NUMBITS(5) []
    ]
];

// Supervisor trap bad address or instruction.
register_bitfields![u64,
    pub stval [
        // How this field is interpreted depends on the trap type.
        // Generally it carries the faulting address for page or
        // alignment faults, and the faulting instruction for illegal
        // instruction faults.
        val OFFSET(0) NUMBITS(64) [],
    ]
];

// Supervisor interrupt pending register.
register_bitfields![u64,
    pub sip [
        ssoft OFFSET(1) NUMBITS(1) [],
        stimer OFFSET(5) NUMBITS(1) [],
        sext OFFSET(9) NUMBITS(1) [],
    ]
];

// Supervisor timer compare register.
register_bitfields![u64,
    pub stimecmp [
        cmp_val OFFSET(0) NUMBITS(64) [],
    ]
];

// Supervisor address translation register.
register_bitfields![u64,
    pub satp [
        // Physical page number of the root translation table.
        ppn OFFSET(0) NUMBITS(44) [],
        // Address-space ID.
        asid OFFSET(44) NUMBITS(16) [],
        // Translation mode.
        mode OFFSET(60) NUMBITS(4) [
            Bare = 0,
            Sv39 = 8,
            Sv48 = 9,
            Sv57 = 10,
            Sv64 = 11,
        ],
    ]
];

// VCoVE: seed CSR layout used for VM emulation.
register_bitfields![u64,
    pub seed [
        entropy OFFSET(0) NUMBITS(16) [],
        custom OFFSET(16) NUMBITS(8) [],
        reserved OFFSET(24) NUMBITS(6) [],
        OPST OFFSET(30) NUMBITS(2) []
    ]
];

// ------------------------------------------------------------------
// Vector extension registers
// ------------------------------------------------------------------

// Vector start position.
register_bitfields![u64,
    pub vstart [
        value OFFSET(0) NUMBITS(64) [],
    ]
];

// Vector control and status register.
register_bitfields![u64,
    pub vcsr [
        value OFFSET(0) NUMBITS(64) [],
    ]
];

// Vector length.
register_bitfields![u64,
    pub vl [
        value OFFSET(0) NUMBITS(64) [],
    ]
];

// Vector data type register.
register_bitfields![u64,
    pub vtype [
        value OFFSET(0) NUMBITS(64) [],
    ]
];

// VLEN/8 (vector register width in bytes).
register_bitfields![u64,
    pub vlenb [
        value OFFSET(0) NUMBITS(64) [],
    ]
];

// ------------------------------------------------------------------
// AIA / IMSIC registers
// ------------------------------------------------------------------

// IMSIC indirect CSR address register.
register_bitfields![u64,
    pub siselect [
        reg_addr OFFSET(0) NUMBITS(64) [],
    ]
];

// IMSIC indirect CSR value register.
register_bitfields![u64,
    pub sireg [
        reg_val OFFSET(0) NUMBITS(64) [],
    ]
];

// External interrupt claim register.
register_bitfields![u64,
    pub stopei [
        interrupt_id OFFSET(16) NUMBITS(11) [],
        interrupt_prio OFFSET(0) NUMBITS(11) [],
    ]
];

// Top-level interrupt claim register.
register_bitfields![u64,
    pub stopi [
        interrupt_id OFFSET(16) NUMBITS(8) [],
        interrupt_prio OFFSET(0) NUMBITS(8) [],
    ]
];

// IMSIC indirect registers.
register_bitfields![u64,
    pub eidelivery [
        value OFFSET(0) NUMBITS(64) [],
    ]
];

register_bitfields![u64,
    pub eithreshold [
        value OFFSET(0) NUMBITS(64) [],
    ]
];

register_bitfields![u64,
    pub eip [
        value OFFSET(0) NUMBITS(64) [],
    ]
];

register_bitfields![u64,
    pub eie [
        value OFFSET(0) NUMBITS(64) [],
    ]
];

// ------------------------------------------------------------------
// Performance counters
// ------------------------------------------------------------------

register_bitfields![u64,
    pub hpmcounter [
        value OFFSET(0) NUMBITS(64) [],
    ]
];
