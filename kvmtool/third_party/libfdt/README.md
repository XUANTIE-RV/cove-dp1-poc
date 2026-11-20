# libfdt — Source and Licensing

## Component

libfdt is a component of the dtc (Device Tree Compiler) project, upstream at
https://github.com/dgibson/dtc. It provides APIs for reading and manipulating flattened device
trees (FDTs).

## Origin

These files were taken from the dtc/libfdt copy carried in the Linux kernel source tree
(`scripts/dtc/libfdt/`), not fetched directly from the dtc upstream repository. The exact Linux
kernel version/commit they came from was not recorded separately.

## File List

The vendored set is the minimal subset needed to build kvmtool:

- C sources: `fdt.c`, `fdt_ro.c`, `fdt_rw.c`, `fdt_strerror.c`, `fdt_sw.c`, `fdt_wip.c`
- Headers: `fdt.h`, `libfdt.h`, `libfdt_env.h`, `libfdt_internal.h`

## License

SPDX identifier: `(GPL-2.0-or-later OR BSD-2-Clause)`. This is a disjunctive dual license:
recipients may use the files under either license and are not required to comply with both
licenses simultaneously.

Full license texts are available in the repository's top-level `LICENSES/` directory.

## Local Modifications

None. The files are used verbatim and contain no local changes.
