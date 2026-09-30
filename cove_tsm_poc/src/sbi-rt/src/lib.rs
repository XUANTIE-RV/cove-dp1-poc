// SPDX-FileCopyrightText: 2026 DAMO Inc.
// SPDX-FileCopyrightText: 2023 Rivos Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Rust SBI message parsing.
//! `SbiMessage` is an enum of all the SBI extensions.
//! For each extension, a function enum is defined to contain the SBI function data.
#![no_std]

mod consts;
pub use consts::*;
/// Error types encapsulating SBI error codes
pub mod error;
pub use error::*;
mod function;
pub use function::*;
// The COVE host SBI extension
mod cove_host;
pub use cove_host::*;
// The COVE interrupt SBI extension
mod cove_interrupt;
pub use cove_interrupt::*;
// The COVE guest SBI extension (COVG)
mod cove_guest;
pub use cove_guest::*;
// The debug console SBI extension (DBCN)
mod dbcn;
pub use dbcn::*;
// The base SBI extension
mod base;
pub use base::*;
// Legacy SBI extensions (EID 0x00-0x08)
pub mod legacy;
// Timer extension (EID 0x54494D45)
mod time;
pub use time::*;
// IPI extension (EID 0x735049)
mod ipi;
pub use ipi::*;
// RFENCE extension (EID 0x52464E43)
mod rfence;
pub use rfence::*;
// HSM extension (EID 0x48534D)
mod hsm;
pub use hsm::*;
// SRST extension (EID 0x53525354)
mod srst;
pub use srst::*;
// PMU extension (EID 0x504D55)
mod pmu;
pub use pmu::*;
// SUSP extension (EID 0x53555350)
mod susp;
pub use susp::*;
// CPPC extension (EID 0x43505043)
mod cppc;
pub use cppc::*;
// FWFT extension (EID 0x46574654)
mod fwft;
pub use fwft::*;
// DBTR extension (EID 0x44425452)
mod dbtr;
pub use dbtr::*;
// MPXY extension (EID 0x4D505859)
mod mpxy;
pub use mpxy::*;
// NACL extension (EID 0x4E41434C)
mod nacl;
pub use nacl::*;
// SSE extension (EID 0x535345)
mod sse;
pub use sse::*;
// STA extension (EID 0x535441)
mod sta;
pub use sta::*;

#[cfg(all(target_arch = "riscv64", target_os = "none"))]
use core::arch::asm;

/// The values returned from an SBI function call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SbiReturn {
    /// The error code (0 for success).
    pub error_code: i64,
    /// The return value if the operation is successful.
    pub return_value: i64,
}

impl SbiReturn {
    /// Returns an `SbiReturn` that indicates success.
    pub fn success(return_value: i64) -> Self {
        Self {
            error_code: SBI_SUCCESS,
            return_value,
        }
    }
}

impl From<Result<i64>> for SbiReturn {
    fn from(result: Result<i64>) -> SbiReturn {
        match result {
            Ok(rv) => Self::success(rv),
            Err(e) => Self::from(e),
        }
    }
}

impl From<Error> for SbiReturn {
    fn from(error: Error) -> SbiReturn {
        SbiReturn {
            error_code: error as i64,
            return_value: 0,
        }
    }
}

impl From<SbiReturn> for Result<i64> {
    fn from(ret: SbiReturn) -> Result<i64> {
        match ret.error_code {
            SBI_SUCCESS => Ok(ret.return_value),
            e => Err(Error::from_code(e)),
        }
    }
}

impl From<SbiReturn> for Result<u64> {
    fn from(ret: SbiReturn) -> Result<u64> {
        match ret.error_code {
            SBI_SUCCESS => Ok(ret.return_value as u64),
            e => Err(Error::from_code(e)),
        }
    }
}

impl From<SbiReturn> for Result<usize> {
    fn from(ret: SbiReturn) -> Result<usize> {
        match ret.error_code {
            SBI_SUCCESS => Ok(ret.return_value as usize),
            e => Err(Error::from_code(e)),
        }
    }
}

impl From<SbiReturn> for Result<()> {
    fn from(ret: SbiReturn) -> Result<()> {
        match ret.error_code {
            SBI_SUCCESS => Ok(()),
            e => Err(Error::from_code(e)),
        }
    }
}

/// SBI return value conventions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SbiReturnType {
    /// Legacy (v0.1) extensions return a single value in A0, usually with the convention that 0
    /// is success and < 0 is an implementation defined error code.
    Legacy(u64),
    /// Modern extensions use the standard error code values enumerated above.
    Standard(SbiReturn),
}

/// SBI Message used to invoke the specified SBI extension in the firmware.
#[derive(Clone, Copy, Debug)]
pub enum SbiMessage {
    /// The legacy PutChar extension.
    PutChar(u64),
    /// The base extension (EID #0x10) for querying SBI version and probing extensions.
    Base(BaseFunction),
    /// Provides capabilities for starting confidential virtual machines.
    CoveHost(CoveHostFunction),
    /// Provides interrupt virtualization for confidential virtual machines.
    CoveInterrupt(CoveInterruptFunction),
    /// Provides guest-side memory sharing and interrupt management for TVM guests.
    CoveGuest(CoveGuestFunction),
    /// The debug console extension (DBCN) for early-boot prints.
    DebugConsole(DbcnFunction),
    /// The timer extension (EID 0x54494D45) for scheduling timer interrupts.
    Timer(TimeFunction),
    /// The IPI extension (EID 0x735049) for inter-processor interrupts.
    Ipi(IpiFunction),
    /// The RFENCE extension (EID 0x52464E43) for remote fence operations.
    Rfence(RfenceFunction),
    /// The HSM extension (EID 0x48534D) for hart state management.
    Hsm(HsmFunction),
    /// The SRST extension (EID 0x53525354) for system reset.
    Srst(SrstFunction),
    /// The PMU extension (EID 0x504D55) for performance monitoring.
    Pmu(PmuFunction),
    /// The SUSP extension (EID 0x53555350) for system suspend.
    Susp(SuspFunction),
    /// The CPPC extension (EID 0x43505043) for processor performance control.
    Cppc(CppcFunction),
    /// The FWFT extension (EID 0x46574654) for firmware feature configuration.
    Fwft(FwftFunction),
    /// The DBTR extension (EID 0x44425452) for debug triggers.
    Dbtr(DbtrFunction),
    /// The MPXY extension (EID 0x4D505859) for message proxy.
    Mpxy(MpxyFunction),
    /// The NACL extension (EID 0x4E41434C) for nested acceleration.
    Nacl(NaclFunction),
    /// The SSE extension (EID 0x535345) for supervisor software events.
    Sse(SseFunction),
    /// The STA extension (EID 0x535441) for steal-time accounting.
    Sta(StaFunction),
    /// The extension for getting vendor.
    Vendor([u64; 8]),
    /// The extension for TEERET.
    TeeRet([u64; 2]),
}

impl SbiMessage {
    /// Creates an SbiMessage struct from the given GPRs. Intended for use from the ECALL handler
    /// and passed the saved register state from the calling OS. A7 must contain a valid SBI
    /// extension and the other A* registers will be interpreted based on the extension A7 selects.
    pub fn from_regs(args: &[u64]) -> Result<Self> {
        match args[7] {
            EXT_PUT_CHAR => Ok(SbiMessage::PutChar(args[0])),
            EXT_BASE => BaseFunction::from_regs(args).map(SbiMessage::Base),
            EXT_COVE_HOST => CoveHostFunction::from_regs(args).map(SbiMessage::CoveHost),
            EXT_COVE_INTERRUPT => {
                CoveInterruptFunction::from_regs(args).map(SbiMessage::CoveInterrupt)
            }
            EXT_COVE_GUEST => CoveGuestFunction::from_regs(args).map(SbiMessage::CoveGuest),
            EXT_DBCN => DbcnFunction::from_regs(args).map(SbiMessage::DebugConsole),
            EXT_TIME => TimeFunction::from_regs(args).map(SbiMessage::Timer),
            EXT_IPI => IpiFunction::from_regs(args).map(SbiMessage::Ipi),
            EXT_RFENCE => RfenceFunction::from_regs(args).map(SbiMessage::Rfence),
            EXT_HSM => HsmFunction::from_regs(args).map(SbiMessage::Hsm),
            EXT_SRST => SrstFunction::from_regs(args).map(SbiMessage::Srst),
            EXT_PMU => PmuFunction::from_regs(args).map(SbiMessage::Pmu),
            EXT_SUSP => SuspFunction::from_regs(args).map(SbiMessage::Susp),
            EXT_CPPC => CppcFunction::from_regs(args).map(SbiMessage::Cppc),
            EXT_FWFT => FwftFunction::from_regs(args).map(SbiMessage::Fwft),
            EXT_DBTR => DbtrFunction::from_regs(args).map(SbiMessage::Dbtr),
            EXT_MPXY => MpxyFunction::from_regs(args).map(SbiMessage::Mpxy),
            EXT_NACL => NaclFunction::from_regs(args).map(SbiMessage::Nacl),
            EXT_SSE => SseFunction::from_regs(args).map(SbiMessage::Sse),
            EXT_STA => StaFunction::from_regs(args).map(SbiMessage::Sta),
            EXT_VENDOR_RANGE_START..=EXT_VENDOR_RANGE_END => Ok(SbiMessage::Vendor(
                args.try_into().map_err(|_| Error::Failed)?,
            )),
            EXT_TEE_RET => Ok(SbiMessage::TeeRet([args[0], args[1]])),
            _ => Err(Error::NotSupported),
        }
    }

    /// Returns the register values `[a0, a1, a2, a3, a4, a5, a6, a7]` for this SBI message.
    pub fn regs(&self) -> [u64; 8] {
        use SbiMessage::*;
        /// Pack an extension function's a0–a6 registers plus its EID into a
        /// single `[u64; 8]` array.  Eliminates the per-variant 8-line block
        /// that was duplicated for every standard extension.
        macro_rules! ext_regs {
            ($f:expr, $ext:expr) => {
                [
                    $f.a0(),
                    $f.a1(),
                    $f.a2(),
                    $f.a3(),
                    $f.a4(),
                    $f.a5(),
                    $f.a6(),
                    $ext,
                ]
            };
        }
        match self {
            PutChar(c) => [*c, 0, 0, 0, 0, 0, 0, EXT_PUT_CHAR],
            Base(f) => ext_regs!(f, EXT_BASE),
            CoveHost(f) => ext_regs!(f, EXT_COVE_HOST),
            CoveInterrupt(f) => ext_regs!(f, EXT_COVE_INTERRUPT),
            CoveGuest(f) => ext_regs!(f, EXT_COVE_GUEST),
            DebugConsole(f) => ext_regs!(f, EXT_DBCN),
            Timer(f) => ext_regs!(f, EXT_TIME),
            Ipi(f) => ext_regs!(f, EXT_IPI),
            Rfence(f) => ext_regs!(f, EXT_RFENCE),
            Hsm(f) => ext_regs!(f, EXT_HSM),
            Srst(f) => ext_regs!(f, EXT_SRST),
            Pmu(f) => ext_regs!(f, EXT_PMU),
            Susp(f) => ext_regs!(f, EXT_SUSP),
            Cppc(f) => ext_regs!(f, EXT_CPPC),
            Fwft(f) => ext_regs!(f, EXT_FWFT),
            Dbtr(f) => ext_regs!(f, EXT_DBTR),
            Mpxy(f) => ext_regs!(f, EXT_MPXY),
            Nacl(f) => ext_regs!(f, EXT_NACL),
            Sse(f) => ext_regs!(f, EXT_SSE),
            Sta(f) => ext_regs!(f, EXT_STA),
            Vendor(regs) => *regs,
            TeeRet(regs) => [regs[0], regs[1], 0, 0, 0, 0, 0, EXT_TEE_RET],
        }
    }

    /// Returns the a0 register value.
    pub fn a0(&self) -> u64 {
        self.regs()[0]
    }
    /// Returns the a1 register value.
    pub fn a1(&self) -> u64 {
        self.regs()[1]
    }
    /// Returns the a2 register value.
    pub fn a2(&self) -> u64 {
        self.regs()[2]
    }
    /// Returns the a3 register value.
    pub fn a3(&self) -> u64 {
        self.regs()[3]
    }
    /// Returns the a4 register value.
    pub fn a4(&self) -> u64 {
        self.regs()[4]
    }
    /// Returns the a5 register value.
    pub fn a5(&self) -> u64 {
        self.regs()[5]
    }
    /// Returns the a6 register value.
    pub fn a6(&self) -> u64 {
        self.regs()[6]
    }
    /// Returns the a7 register value.
    pub fn a7(&self) -> u64 {
        self.regs()[7]
    }

    /// Returns the result returned in the SbiMessage. Intended for use after an SbiMessage has been
    /// handled by the firmware. Interprets the given registers based on the extension and function
    /// and returns the appropriate result.
    ///
    /// # Example
    ///
    /// ```rust
    /// #[cfg(all(target_arch = "riscv64", target_os = "none"))]
    /// pub fn ecall_send(msg: &SbiMessage) -> Result<u64> {
    ///     let mut a0 = msg.a0(); // error code
    ///     let mut a1 = msg.a1(); // return value
    ///     unsafe {
    ///         // Safe, but relies on trusting the hypervisor or firmware.
    ///         asm!("ecall", inout("a0") a0, inout("a1")a1,
    ///                 in("a2")msg.a2(), in("a3") msg.a3(),
    ///                 in("a4")msg.a4(), in("a5") msg.a5(),
    ///                 in("a6")msg.a6(), in("a7") msg.a7());
    ///     }
    ///
    ///     msg.result(a0, a1)
    /// }
    /// ```
    pub fn result<T>(&self, a0: i64, a1: i64) -> Result<T>
    where
        Result<T>: From<SbiReturn>,
    {
        let ret = SbiReturn {
            error_code: a0,
            return_value: a1,
        };
        match self {
            // For legacy messages, a0 is 0 on success and an implementation-defined error value on
            // failure. Nothing is returned in a1.
            SbiMessage::PutChar(_) => match a0 {
                SBI_SUCCESS => SbiReturn {
                    error_code: 0,
                    return_value: 0,
                }
                .into(),
                _ => Err(Error::Failed),
            },
            _ => ret.into(),
        }
    }
}

/// Sends an ecall to the firmware or hypervisor.
///
/// # Safety
///
/// The caller must verify that any memory references contained in `msg` obey Rust's memory
/// safety rules. For example, any pointers to memory that will be modified in the handling of
/// the ecall must be uniquely owned. Similarly any pointers read by the ecall must not be
/// mutably borrowed.
///
/// In addition the caller is placing trust in the firmware or hypervisor to maintain the promises
/// of the interface w.r.t. reading and writing only within the provided bounds.
#[cfg(all(target_arch = "riscv64", target_os = "none"))]
pub unsafe fn ecall_send<T>(msg: &SbiMessage) -> Result<T>
where
    Result<T>: From<SbiReturn>,
{
    // normally error code
    let mut a0;
    // normally return value
    let mut a1;
    asm!("ecall", inlateout("a0") msg.a0()=>a0, inlateout("a1")msg.a1()=>a1,
                in("a2")msg.a2(), in("a3") msg.a3(),
                in("a4")msg.a4(), in("a5") msg.a5(),
                in("a6")msg.a6(), in("a7") msg.a7(), options(nostack));

    msg.result(a0, a1)
}

/// Forward a raw SBI ecall with explicit register values (a0–a7).
///
/// Returns `(a0, a1)` — the raw error code and return value from the firmware.
/// This is used by the TSM to forward standard SBI calls from TVM guests
/// to the RDSM/OpenSBI without parsing through `SbiMessage`.
///
/// # Safety
///
/// The caller must ensure the register values form a valid SBI call and that
/// any memory references are safe to pass to the firmware.
#[allow(clippy::too_many_arguments)]
#[cfg(all(target_arch = "riscv64", target_os = "none"))]
pub unsafe fn ecall_forward_raw(
    a0: u64,
    a1: u64,
    a2: u64,
    a3: u64,
    a4: u64,
    a5: u64,
    a6: u64,
    a7: u64,
) -> (i64, i64) {
    let ret_a0: i64;
    let ret_a1: i64;
    asm!("ecall",
        inlateout("a0") a0 => ret_a0,
        inlateout("a1") a1 => ret_a1,
        in("a2") a2, in("a3") a3,
        in("a4") a4, in("a5") a5,
        in("a6") a6, in("a7") a7,
        options(nostack),
    );
    (ret_a0, ret_a1)
}

/// Compilation stub for non-bare-metal targets — panics if called.
///
/// # Safety
///
/// Do not call. Only present so the crate compiles on non-RISC-V hosts.
#[cfg(not(all(target_arch = "riscv64", target_os = "none")))]
pub unsafe fn ecall_send<T>(_msg: &SbiMessage) -> Result<T>
where
    Result<T>: From<SbiReturn>,
{
    panic!("ecall_send called on non-riscv64 target");
}

/// Compilation stub for non-bare-metal targets — panics if called.
///
/// # Safety
///
/// Do not call. Only present so the crate compiles on non-RISC-V hosts.
#[allow(clippy::too_many_arguments)]
#[cfg(not(all(target_arch = "riscv64", target_os = "none")))]
pub unsafe fn ecall_forward_raw(
    _a0: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
    _a4: u64,
    _a5: u64,
    _a6: u64,
    _a7: u64,
) -> (i64, i64) {
    panic!("ecall_forward_raw called on non-riscv64 target");
}
