#!/bin/bash
#
# build.sh - Unified entry point for the CoVE DP-1 PoC
#
# Usage:
#   ./build.sh tsm    Build the TSM firmware from source into build/tsm.bin
#   ./build.sh run    Launch QEMU with the built TSM and the runtime bundle

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

RISCV_TARGET="riscv64gc-unknown-none-elf"
RELEASE_DIR="${SCRIPT_DIR}/cove-dp1-poc-runtime-rv64"
TSM_BIN="${SCRIPT_DIR}/build/tsm.bin"
GCC_PREFIX="riscv64-unknown-linux-gnu-"

# --- Toolchain discovery ------------------------------------------------------

discover_gcc() {
    if [[ -n "${RISCV_TOOLCHAIN_BIN:-}" ]] && [[ -x "${RISCV_TOOLCHAIN_BIN}/${GCC_PREFIX}gcc" ]]; then
        return 0
    fi
    if command -v "${GCC_PREFIX}gcc" &>/dev/null; then
        RISCV_TOOLCHAIN_BIN="$(dirname "$(command -v "${GCC_PREFIX}gcc")")"
        return 0
    fi
    echo "Error: ${GCC_PREFIX}gcc not found."
    echo "  Option 1: export RISCV_TOOLCHAIN_BIN=<dir containing ${GCC_PREFIX}gcc>"
    echo "  Option 2: Add the toolchain bin directory to PATH"
    echo "  See the Prerequisites section in README.md for how to obtain the toolchain."
    exit 1
}

discover_qemu() {
    if [[ -n "${QEMU_BIN:-}" ]] && [[ -x "${QEMU_BIN}/qemu-system-riscv64" ]]; then
        return 0
    fi
    if command -v qemu-system-riscv64 &>/dev/null; then
        QEMU_BIN="$(dirname "$(command -v qemu-system-riscv64)")"
        return 0
    fi
    echo "Error: qemu-system-riscv64 not found."
    echo "  Option 1: export QEMU_BIN=<dir containing qemu-system-riscv64>"
    echo "  Option 2: Add QEMU to PATH"
    echo "  See the Prerequisites section in README.md for how to obtain the Smmpt-capable XuanTie QEMU."
    exit 1
}

# --- tsm: build the TSM firmware from the source workspace --------------------

build_tsm() {
    if ! command -v cargo &>/dev/null; then
        echo "Error: cargo not found. Please install Rust: https://rustup.rs/"
        exit 1
    fi
    discover_gcc

    local objcopy="${RISCV_TOOLCHAIN_BIN}/${GCC_PREFIX}objcopy"
    local objdump="${RISCV_TOOLCHAIN_BIN}/${GCC_PREFIX}objdump"
    if [[ ! -x "${objcopy}" || ! -x "${objdump}" ]]; then
        echo "Error: ${GCC_PREFIX}objcopy/objdump not found in ${RISCV_TOOLCHAIN_BIN}."
        echo "  The RISC-V GCC toolchain must provide gcc, objcopy and objdump."
        exit 1
    fi

    echo "[CoVE DP-1] Building TSM from source..."
    # The build must run with xvm2/ as the working directory:
    # rustup resolves the pinned nightly toolchain from
    # xvm2/rust-toolchain.toml based on the current directory, while
    # Cargo discovers the workspace-level .cargo/config.toml located at
    # xvm2/.cargo/config.toml relative to that same directory.
    (
        cd xvm2
        PATH="${RISCV_TOOLCHAIN_BIN}:${PATH}" \
            cargo build \
            --release --target "${RISCV_TARGET}" \
            --features riscv-bare \
            -Z build-std=core,alloc,compiler_builtins \
            -Z build-std-features=compiler-builtins-mem
    )

    local elf="xvm2/target/${RISCV_TARGET}/release/tsm"
    mkdir -p build
    cp "$elf" build/tsm.elf
    "${objcopy}" -O binary "$elf" build/tsm.bin
    "${objdump}" -d "$elf" > build/tsm.dump

    echo "[CoVE DP-1] TSM built: ${TSM_BIN}"
}

# --- run: launch QEMU with the built TSM and the runtime bundle ---------------

run() {
    discover_qemu

    local runtime_files=(
        "${RELEASE_DIR}/fw_dynamic.bin"
        "${RELEASE_DIR}/Image"
        "${RELEASE_DIR}/lkvm-static"
        "${RELEASE_DIR}/rootfs.xuantie-image-rv64-6.6-lite.ext4"
    )
    for file in "${runtime_files[@]}"; do
        if [[ ! -f "$file" ]]; then
            echo "Error: runtime file not found in '${RELEASE_DIR}/'."
            echo "  Extract the cove-dp1-poc-runtime-rv64.tar.gz GitHub Release asset at the repository root (creates ${RELEASE_DIR}/) first."
            exit 1
        fi
    done

    if [[ ! -f "${TSM_BIN}" ]]; then
        echo "Error: ${TSM_BIN} not found."
        echo "  Run './build.sh tsm' first to build TSM from source."
        exit 1
    fi

    echo "[CoVE DP-1] Starting QEMU with build/tsm.bin ..."
    PATH="${QEMU_BIN}:${PATH}" qemu-system-riscv64 \
        -nographic -smp 1 \
        -M virt,aia=aplic-imsic,aia-guests=4 \
        -cpu xt-c9501fdvkt -m 4G \
        -append 'rootwait root=/dev/vda rw ip=dhcp' \
        -device virtio-net-device,netdev=net0 \
        -netdev "user,hostfwd=tcp::22222-:22,id=net0" \
        -bios "${RELEASE_DIR}/fw_dynamic.bin" \
        -kernel "${RELEASE_DIR}/Image" \
        -device guest-loader,kernel="${TSM_BIN}",addr=0xA0200000 \
        -device virtio-blk-pci,drive=hd0 \
        -drive "file=${RELEASE_DIR}/rootfs.xuantie-image-rv64-6.6-lite.ext4,format=raw,id=hd0,snapshot=on"
}

usage() {
    echo "Usage: $0 {tsm|run}"
    echo ""
    echo "Commands:"
    echo "  tsm    Build the TSM firmware from source into build/tsm.bin"
    echo "         (requires only the Rust and RISC-V GCC toolchains)"
    echo "  run    Launch QEMU with the built TSM and the runtime bundle"
    echo "         (checks QEMU, cove-dp1-poc-runtime-rv64/ and build/tsm.bin;"
    echo "          never rebuilds TSM implicitly)"
    exit 1
}

# --- Main ---------------------------------------------------------------------

case "${1:-}" in
    tsm) build_tsm ;;
    run) run ;;
    *)   usage ;;
esac
