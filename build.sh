#!/bin/bash
#
# build.sh - Unified entry point for the CoVE DP-1 PoC
#
# Usage:
#   ./build.sh tsm    Build the TSM firmware from source into build/tsm.bin
#   ./build.sh lkvm   Cross-build lkvm-static from kvmtool/ into build/
#   ./build.sh run    Launch QEMU with the built TSM and the runtime bundle
#   ./build.sh clean  Remove all build artifacts (kvmtool objects, libfdt,
#                     build/ outputs) so the next build starts from scratch

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

RISCV_TARGET="riscv64gc-unknown-none-elf"
RELEASE_DIR="${SCRIPT_DIR}/cove-dp1-poc-runtime-rv64"
TSM_BIN="${SCRIPT_DIR}/build/tsm.bin"
GCC_PREFIX="riscv64-unknown-linux-gnu-"

LKVM_DIR="${SCRIPT_DIR}/kvmtool"
LKVM_ARCH="riscv"
LKVM_BIN="${SCRIPT_DIR}/build/lkvm-static"
LKVM_LIBFDT_SRC="${LKVM_DIR}/third_party/libfdt"
LKVM_LIBFDT_BUILD="${SCRIPT_DIR}/build/lkvm-libfdt"
# The RISC-V ABIs are pinned explicitly: kvmtool picks them up from
# RISCV_XLEN=64, and the vendored libfdt must be compiled with the same ones
# instead of relying on the toolchain default.
LKVM_ARCH_FLAGS=(-march=rv64gc -mabi=lp64d)

# --- Toolchain discovery ------------------------------------------------------

discover_gcc() {
    if [[ -n "${RISCV_TOOLCHAIN_BIN:-}" ]] && [[ -x "${RISCV_TOOLCHAIN_BIN}/${GCC_PREFIX}gcc" ]]; then
        RISCV_TOOLCHAIN_BIN="$(cd -- "${RISCV_TOOLCHAIN_BIN}" && pwd -P)"
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

# --- lkvm: cross-build lkvm-static from the in-tree kvmtool sources -----------
#
# Identity/path sanitization flags. The kvmtool Makefile appends CPPFLAGS into
# its own CFLAGS, so CPPFLAGS is the supported injection point: assigning CFLAGS
# on the make command line would clobber the in-Makefile flags (e.g. -march=rv64gc
# and the -Werror set), while CPPFLAGS only adds to them.
LKVM_SANITIZE_FLAGS=(
    -fno-ident
    "-ffile-prefix-map=${LKVM_DIR}=."
    "-ffile-prefix-map=${SCRIPT_DIR}=."
    "-fdebug-prefix-map=${LKVM_DIR}=."
    "-fdebug-prefix-map=${SCRIPT_DIR}=."
)

build_lkvm_libfdt() {
    local cc="$1" ar="$2" src obj

    if [[ ! -d "${LKVM_LIBFDT_SRC}" ]]; then
        echo "Error: vendored libfdt sources not found in ${LKVM_LIBFDT_SRC}."
        echo "  kvmtool/third_party/libfdt must ship with this repository; lkvm cannot be built without it."
        exit 1
    fi

    echo "[CoVE DP-1] Building libfdt from kvmtool/third_party/libfdt ..."
    rm -rf "${LKVM_LIBFDT_BUILD}"
    mkdir -p "${LKVM_LIBFDT_BUILD}"

    for src in "${LKVM_LIBFDT_SRC}"/*.c; do
        obj="${LKVM_LIBFDT_BUILD}/$(basename "${src%.c}").o"
        "${cc}" -c -O2 -fno-strict-aliasing "${LKVM_ARCH_FLAGS[@]}" \
            "${LKVM_SANITIZE_FLAGS[@]}" -I"${LKVM_LIBFDT_SRC}" "${src}" -o "${obj}"
    done

    "${ar}" rcs "${LKVM_LIBFDT_BUILD}/libfdt.a" "${LKVM_LIBFDT_BUILD}"/*.o
    cp "${LKVM_LIBFDT_SRC}"/*.h "${LKVM_LIBFDT_BUILD}/"
    echo "[CoVE DP-1] libfdt built: ${LKVM_LIBFDT_BUILD}/libfdt.a"
}

build_lkvm() {
    discover_gcc

    # Absolute tool prefix. discover_gcc() has already resolved the toolchain
    # directory, so handing the full "<dir>/<prefix>" to make is enough: the
    # kvmtool Makefile defines CC := $(CROSS_COMPILE)gcc and the vendored libfdt
    # is compiled with the same absolute CC. No PATH manipulation is involved,
    # so the build can never silently pick up a different toolchain.
    local cross="${RISCV_TOOLCHAIN_BIN}/${GCC_PREFIX}"
    local cc="${cross}gcc"
    local ar="${cross}ar"
    local strip_bin="${cross}strip"
    local tool
    for tool in "${cc}" "${ar}" "${strip_bin}"; do
        if [[ ! -x "${tool}" ]]; then
            echo "Error: ${tool} not found."
            echo "  Building lkvm-static requires a RISC-V GCC toolchain providing"
            echo "  gcc, ar and strip with the ${GCC_PREFIX} prefix."
            echo "  Set RISCV_TOOLCHAIN_BIN to the directory that holds these tools."
            exit 1
        fi
    done

    if [[ ! -f "${LKVM_DIR}/Makefile" ]]; then
        echo "Error: kvmtool sources not found in ${LKVM_DIR}."
        echo "  kvmtool/ must contain the in-tree kvmtool sources shipped with this repository."
        exit 1
    fi

    build_lkvm_libfdt "${cc}" "${ar}"

    echo "[CoVE DP-1] Building lkvm-static (ARCH=${LKVM_ARCH}) ..."
    make -C "${LKVM_DIR}" \
        ARCH="${LKVM_ARCH}" \
        RISCV_XLEN=64 \
        CROSS_COMPILE="${cross}" \
        LIBFDT_DIR="${LKVM_LIBFDT_BUILD}" \
        CPPFLAGS="${LKVM_SANITIZE_FLAGS[*]} -Wno-discarded-qualifiers" \
        lkvm-static

    mkdir -p "${SCRIPT_DIR}/build"
    cp "${LKVM_DIR}/lkvm-static" "${LKVM_BIN}"
    "${strip_bin}" "${LKVM_BIN}"
    echo "[CoVE DP-1] lkvm-static built: ${LKVM_BIN}"
    echo "            (the runtime bundle is not touched; deploy the binary manually when packaging a new bundle)"
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

# --- clean: remove generated build artifacts ---------------------------------
#
# 'make -C kvmtool clean' cannot be used here. The kvmtool Makefile resolves
# libfdt while it parses (config/feature-tests.mak + the LIBFDT_DIR probing),
# so without a populated LIBFDT_DIR it aborts with "No libfdt found", and with
# an empty LIBFDT_DIR with "libfdt.a not found" - the clean rule is never
# reached. The artifacts are therefore removed explicitly.
#
# Only compiler byproducts and generated files are deleted. Paths that git
# tracks are skipped, so local source modifications survive a clean.
#
# This command deliberately does not call discover_gcc(): cleaning up after a
# toolchain switch must work even when the new toolchain is not set up yet.

clean_kvmtool() {
    local abs rel count=0 skipped=0
    local -A tracked=()

    if git -C "${SCRIPT_DIR}" rev-parse --git-dir >/dev/null 2>&1; then
        while IFS= read -r -d '' rel; do
            tracked["${rel}"]=1
        done < <(git -C "${SCRIPT_DIR}" ls-files -z -- kvmtool)
    fi

    # '*.o' also matches '*.static.o'; '*.d' also matches the hidden '.x.o.d'
    # dependency files and '*.o.d'.
    while IFS= read -r -d '' abs; do
        rel="${abs#"${SCRIPT_DIR}/"}"
        if [[ -n "${tracked[${rel}]:-}" ]]; then
            echo "[CoVE DP-1]   keep (tracked source): ${rel}"
            skipped=$((skipped + 1))
            continue
        fi
        rm -f -- "${abs}"
        count=$((count + 1))
    done < <(find "${LKVM_DIR}" -type f \
                \( -name '*.o' -o -name '*.d' -o -name '*.o.d' \) -print0)

    # Link products and files generated by the kvmtool/guest build.
    local generated=(
        "${LKVM_DIR}/lkvm-static"
        "${LKVM_DIR}/lkvm"
        "${LKVM_DIR}/vm"
        "${LKVM_DIR}/KVMTOOLS-VERSION-FILE"
        "${LKVM_DIR}/guest/guest_init.c"
        "${LKVM_DIR}/guest/guest_pre_init.c"
        "${LKVM_DIR}/guest/init"
    )
    for abs in "${generated[@]}"; do
        if [[ -e "${abs}" || -L "${abs}" ]]; then
            rm -f -- "${abs}"
            count=$((count + 1))
        fi
    done

    echo "[CoVE DP-1] kvmtool: removed ${count} artifact(s), ${skipped} tracked file(s) kept"
}

clean_build() {
    local path

    echo "[CoVE DP-1] Cleaning build artifacts ..."

    if [[ -d "${LKVM_DIR}" ]]; then
        clean_kvmtool
    else
        echo "[CoVE DP-1]   kvmtool/ not present, skipping"
    fi

    # libfdt objects, the published lkvm-static and the TSM outputs copied out
    # of the cargo target directory by './build.sh tsm'.
    local outputs=(
        "${LKVM_LIBFDT_BUILD}"
        "${LKVM_BIN}"
        "${SCRIPT_DIR}/build/tsm.bin"
        "${SCRIPT_DIR}/build/tsm.elf"
        "${SCRIPT_DIR}/build/tsm.dump"
    )
    for path in "${outputs[@]}"; do
        if [[ -e "${path}" || -L "${path}" ]]; then
            rm -rf -- "${path}"
            echo "[CoVE DP-1]   removed ${path#"${SCRIPT_DIR}/"}"
        fi
    done

    echo "[CoVE DP-1] Clean done; the next './build.sh tsm|lkvm' rebuilds from scratch."
    echo "            Note: xvm2/target (cargo cache) is kept — cargo invalidates on"
    echo "            Rust toolchain changes. After switching the C cross-compiler,"
    echo "            run 'rm -rf xvm2/target' manually if a full TSM rebuild is needed."
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
    echo "Usage: $0 {tsm|lkvm|run|clean}"
    echo ""
    echo "Commands:"
    echo "  tsm    Build the TSM firmware from source into build/tsm.bin"
    echo "         (requires only the Rust and RISC-V GCC toolchains)"
    echo "  lkvm   Cross-build lkvm-static from kvmtool/ into build/lkvm-static"
    echo "         (requires only the RISC-V GCC toolchain; libfdt is vendored"
    echo "          under kvmtool/third_party/libfdt; the runtime bundle is"
    echo "          never modified - deploy the binary manually)"
    echo "  run    Launch QEMU with the built TSM and the runtime bundle"
    echo "         (checks QEMU, cove-dp1-poc-runtime-rv64/ and build/tsm.bin;"
    echo "          never rebuilds TSM or lkvm implicitly)"
    echo "  clean  Remove all build artifacts: kvmtool objects/dependency files and"
    echo "         link products, build/lkvm-libfdt/ and build/{lkvm-static,tsm.*}"
    echo "         (tracked sources are never touched; no toolchain required)"
    exit 1
}

# --- Main ---------------------------------------------------------------------

case "${1:-}" in
    tsm)   build_tsm ;;
    lkvm)  build_lkvm ;;
    run)   run ;;
    clean) clean_build ;;
    *)     usage ;;
esac
