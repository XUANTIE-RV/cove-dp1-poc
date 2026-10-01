/*
 * SPDX-License-Identifier: BSD-2-Clause
 *
 * Copyright (c) 2026 DAMO Inc.
 *
 * build.rs - Cargo build script for linker configuration
 *
 * Configures the linker script path and tracks assembly source files
 * for incremental rebuild.
 */

use std::env;
use std::path::PathBuf;

fn main() {
    let target = env::var("TARGET").unwrap_or_default();
    if target == "riscv64gc-unknown-none-elf" {
        let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
        let lds_path = manifest_dir.join("tsm.lds");

        println!("cargo:rustc-link-arg=-T{}", lds_path.display());
        println!("cargo:rerun-if-changed={}", lds_path.display());
        println!("cargo:rerun-if-changed=src/arch/riscv/asm");
    }
}
