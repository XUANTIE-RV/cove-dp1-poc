# CoVE DP-1 PoC — RISC-V Confidential VM Demo Environment

A CoVE DP-1 demo environment based on QEMU (RISC-V RV64GC), built from the open-source TSM source code in this repository and a runtime bundle distributed separately via GitHub Release.

## Overview

This repository provides a demo environment for **CoVE DP-1**, the RISC-V Confidential VM Extension (CoVE v0.7) deployment module one, together with the open-source reference implementation of the **TSM (TEE Security Manager)**. Building and launching the demo is driven by one unified entry script, `build.sh`, at the repository root.

The TSM runs at HS-mode under a machine-mode RDSM (Root Domain Security Manager) and manages the lifecycle of Trusted Virtual Machines (TVM). And it is written in Rust as a single Cargo workspace under the `xvm2/` subdirectory.

The remaining runtime components — the OpenSBI firmware (`fw_dynamic.bin`), the Host Linux kernel (`Image`), kvmtool (`lkvm-static`) and the root filesystem image — are distributed separately as the `cove-dp1-poc-runtime-rv64.tar.gz` GitHub Release asset. After extraction they reside in `cove-dp1-poc-runtime-rv64/` at the repository root; that directory is local-only and is **not** part of the source repository.

> **Important Notice**
>
> - The runtime binaries (OpenSBI firmware, Host Linux kernel, kvmtool and rootfs) are **not** part of this source repository. Download the `cove-dp1-poc-runtime-rv64.tar.gz` asset from the GitHub Release page and extract it at the repository root before running the demo. See [Download the Runtime Bundle](#download-the-runtime-bundle).
> - Running this PoC requires a Smmpt-capable XuanTie QEMU, which is provided separately as a GitHub Release asset of this repository (see [Prerequisites](#prerequisites)).
> - This project and its accompanying Release assets are intended solely as a readable proof-of-concept reference for CoVE host-side integration and end-to-end validation. They are not production-ready and must not be used in production environments.

## Getting Started

### Prerequisites

- **Host Environment**: Ubuntu 22.04 LTS (x86_64). Other Linux distributions may work but have not been tested.

- **Rust nightly-2025-04-04** (pinned in `xvm2/rust-toolchain.toml`)

  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  source "$HOME/.cargo/env"
  rustup toolchain install nightly-2025-04-04 --component rust-src --target riscv64gc-unknown-none-elf
  ```

- **RISC-V GCC Cross-Compilation Toolchain**

  Requires the `riscv64-unknown-linux-gnu-` prefixed `gcc`, `objcopy` and
  `objdump`. Precompiled releases are available on the
  [riscv-gnu-toolchain releases page](https://github.com/riscv-collab/riscv-gnu-toolchain/releases);
  pick the **riscv64-glibc** build.

  Extract it wherever you prefer, then add the toolchain's `bin/` directory
  to `PATH`:

  ```bash
  export PATH="/path/to/riscv-gnu-toolchain/bin:$PATH"
  ```

- **XuanTie QEMU** (cskysim V5.4.3 or later)

  This proof of concept (PoC) depends on Smmpt v0.3.3 (https://github.com/riscv/riscv-smmtt) related CSRs. These extensions are not yet supported by upstream community QEMU. As of QEMU 11.x, the relevant implementation patches ([Smmpt v8 series](https://lore.kernel.org/qemu-devel/20260723200325.24969-1-zhiwei_liu@linux.alibaba.com/)) are still under community review. Therefore, running this PoC requires a Smmpt-capable XuanTie QEMU (cskysim V5.4.3 or later).

  Download `xuantie-qemu-riscv64-x86_64-linux-V5.4.3.tar.gz` from the GitHub Release page of this repository, then extract it at the repository root:

  ```bash
  tar xzf xuantie-qemu-riscv64-x86_64-linux-V5.4.3.tar.gz
  ```

  Set the `QEMU_BIN` environment variable to the directory that contains the
  `qemu-system-riscv64` binary (the directory itself, not the full path to the
  binary):

  ```bash
  export QEMU_BIN="$(pwd)/xuantie-qemu-riscv64-x86_64-linux/bin"
  ```

  Alternatively, add the same `bin/` directory to your `PATH` environment
  variable:

  ```bash
  export PATH="$(pwd)/xuantie-qemu-riscv64-x86_64-linux/bin:$PATH"
  ```

### Download the Runtime Bundle

The runtime binaries are distributed separately from the source repository:

1. Download `cove-dp1-poc-runtime-rv64.tar.gz` from the GitHub Release page of this repository.
2. Extract it at the repository root:

   ```bash
   tar xzf cove-dp1-poc-runtime-rv64.tar.gz
   ```

   This creates the `cove-dp1-poc-runtime-rv64/` directory containing the four
   runtime files required by `./build.sh run`: `fw_dynamic.bin`, `Image`,
   `lkvm-static` and `rootfs.xuantie-image-rv64-6.6-lite.ext4`.

`cove-dp1-poc-runtime-rv64/` is a local directory obtained from the GitHub Release
asset; it is excluded from the source repository via `.gitignore` and must
never be committed.

### Build the TSM from Source

```bash
./build.sh tsm
```

Cross-compiles the TSM firmware from the Cargo workspace under `xvm2/`.
Output artifacts in `build/`:

- `tsm.bin` — raw firmware binary (loaded by `./build.sh run`)
- `tsm.elf` — ELF executable
- `tsm.dump` — disassembly

### Launch QEMU

```bash
./build.sh run
```

Verifies that QEMU, the four runtime files under
`cove-dp1-poc-runtime-rv64/` and the previously built `build/tsm.bin` are all
present — it never rebuilds TSM implicitly — and then launches QEMU.

After QEMU finishes booting, type at the login prompt:

```
root
```

This will enter the Host Linux Shell.

> **Note:** You must run `./build.sh tsm` to build TSM first; otherwise `run`
> will report an error.

### Demo Test Items

#### 1. Launch Guest TVM

After entering the Host Shell, run `ls` to see the TVM launch scripts.

##### Single TVM — 9P Mode (Recommended)

```bash
./run_tvm.sh
```

Launches a Guest TVM with the filesystem mounted via 9P. The actual lkvm
command executed:

```bash
./lkvm-static run -c1 -m 256 -p 'earlycon=sbi root=/dev/root swiotlb=2048' --cove-vm -k ./Image
```

##### Single TVM — External rootfs Mode

```bash
./run_tvm_with_rootfs.sh
```

Launches a Guest TVM with a standalone rootfs image. The actual lkvm command
executed:

```bash
./lkvm-static run -c1 --console virtio --cove-vm \
    -p "earlycon=sbi console=hvc1" \
    -k ./Image \
    --virtio-transport=pci \
    -d rootfs.xuantie-image-rv64-6.6-lite.ext4
```

> **Note:** The current open-source TSM only supports a single TVM / single
> vCPU. The above `run_tvm.sh` uses `-c1` for single vCPU mode.

#### 2. OpenSSL Test

Execute the following command inside the TVM to test AES algorithm:

```bash
openssl speed -evp aes-128-ecb -seconds 1
```

**Expected result:** Outputs AES algorithm performance metrics (e.g., bytes
processed per second).

#### 3. Confidentiality Test (MPT — Memory Protection Table)

`0xA0200000` is the load address of the TSM, which resides in the confidential
memory domain. The following tests verify that the Host is unable to read from
or write to this protected region.

##### Host Read Confidential Domain Test

`memtool` is pre-installed in the rootfs and can be used directly.

Execute on the Host side:

```bash
memtool md 0xA0200000
```

**Expected result:** Access fails, triggering a Load access fault, cause=5.

##### Host Write Confidential Domain Test

Execute on the Host side:

```bash
memtool mw 0xA0200000 0x2
```

**Expected result:** Write fails, triggering a Store/AMO access fault,
cause=7.

##### Confidential Memory Isolation Lifecycle Test

This test verifies that memory pages converted into the confidential domain
via ConvertPages (`sbi_covh_convert_pages`, COVH FID=1) are protected by MPT
while the TVM is running, and that they return to the normal domain after the
TVM is destroyed.

The ideal full test flow has three steps: access the address from the Host
before TVM launch (accessible) → access it again while the TVM is running
(inaccessible) → access it again after the TVM is released (accessible again).
However, since the Host cannot predict which physical address KVM will
allocate for the TVM in advance, the first step cannot be performed. The test
therefore starts from the second step.

**Step 1: Host accesses a confidential page while the TVM is running**

After launching the TVM, observe the TSM log for a `TsmConvertPages` message
and note the physical address printed. Then execute on the Host side:

```bash
memtool md <address from the log>
```

**Expected result:** Access fails, triggering a Load access fault, cause=5.
This confirms that the page has been marked as confidential by MPT and is
inaccessible to the Host.

**Step 2: Host accesses the same address after the TVM is released**

After the TVM exits or is destroyed, execute on the Host side:

```bash
memtool md <same address>
```

**Expected result:** Access succeeds and returns memory data normally. This
confirms that the page has been returned from the confidential domain to the
normal domain after TVM release, and the Host can access it again.

## Directory Structure

| Path | Description |
|------|------|
| `build.sh` | Unified entry script, supports `tsm` (build) / `run` (launch) commands |
| `CHANGELOG.md` | Release changelog |
| `xvm2/` | TSM (TEE Security Manager) open-source Rust code — a single Cargo workspace |
| `LICENSES/` | Collected license texts: `BSD-2-Clause.txt` and `Apache-2.0.txt` |
| `README.md` | This documentation file |

## System Architecture

The software stack of the CoVE DP-1 demo environment is layered as follows:

```
┌─────────────────────────────────────────────────────────┐
│  Guest TVM (VS/VU-mode)                                 │
│  Confidential Linux + Application Workloads             │
│  (9P rootfs / ext4 rootfs)                              │
├─────────────────────────────────────────────────────────┤
│  TSM – TEE Security Manager (HS-mode)                   │  ← this repository (xvm2/)
│  TVM lifecycle, G-stage paging, SBI COVH/COVI/COVG      │
├─────────────────────────────────────────────────────────┤
│  Host Linux + KVM (HS-mode)                             │  ← cove-dp1-poc-runtime-rv64/Image
│  Non-confidential VM management, lkvm-static            │  ← cove-dp1-poc-runtime-rv64/lkvm-static
├─────────────────────────────────────────────────────────┤
│  RDSM – Root Domain Security Manager                    │  ← cove-dp1-poc-runtime-rv64/fw_dynamic.bin
│  OpenSBI (M-mode), MPT enforcement                      │
├─────────────────────────────────────────────────────────┤
│  Hardware: RISC-V RV64GC + CoVE Extensions              │
│  QEMU (qemu-system-riscv64)                             │
└─────────────────────────────────────────────────────────┘
```

- **TSM** is the only open-source component, running in HS-mode, responsible
  for managing TVM lifecycle and memory isolation
- **RDSM** (OpenSBI) runs in M-mode, enforcing physical memory domain
  isolation via MPT
- **Host Linux + KVM** interacts with TSM through the SBI COVH extension to
  create and manage confidential virtual machines
- **Guest TVM** runs in an isolated memory domain; the Host cannot read or
  write its confidential memory

**Specification Reference**

- [RISC-V CoVE (AP-TEE) Specification v0.7](https://github.com/riscv-non-isa/riscv-ap-tee)
- [RISC-V Smmtt Specification v0.3.3](https://github.com/riscv/riscv-smmtt)

## Platform Specifications

| **Component** | **Specification** |
| --- | --- |
| Execution Platform | RISC-V RV64GC on QEMU |
| Host Kernel | XuanTie open-source kernel v6.6.36 + CoVE Host patches |
| Guest Kernel | XuanTie open-source kernel v6.6.36 + CoVE Guest patches |
| TSM | Rust-based TEE Security Manager (CoVE 0.7 compliant) |
| RDSM | OpenSBI extension for domain switching and MPT management |
| KVM TOOL | LKVM (lightweight KVM tool) |
| RootFS | 9P virtio-fs / custom user-provided rootfs |
| TVM Count | 1 |
| vCPU per TVM | 1 |
| Interrupt Controller | AIA (IMSIC only) |
| Networking | virtio-net (Guest ↔ Host) |
| Memory Isolation | Hardware-enforced MPT |

## Contributors

Listed in alphabetical order:

| Name |
|------|
| Baolong Duan |
| Ruoqing He |
| Xiaoxia Cui |
| Xiangyi Zeng |
