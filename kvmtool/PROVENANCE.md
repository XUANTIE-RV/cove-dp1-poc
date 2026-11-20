# Source Provenance

This document records where the kvmtool sources in this directory come from and how they were
adapted. It covers the upstream baseline, the CoVE reference implementation, the downstream
XuanTie changes, and the applicable licensing terms.

## Upstream Baseline

The bulk of the tree is derived from the upstream kvmtool project at
https://github.com/kvmtool/kvmtool.

The baseline is commit
[`da4cfc3e540341b84c4bbad705b5a15865bc1f80`](https://github.com/kvmtool/kvmtool/commit/da4cfc3e540341b84c4bbad705b5a15865bc1f80)
(2024-03-30, "x86: Fix some memory sizes when setting up bios"). Files added downstream for the
CoVE work are not part of that baseline and are described below.

## CoVE Reference Implementation

The CoVE-specific guest support is based on Rivos' kvmtool fork, branch
`cove-integration-03072023`, whose HEAD is commit
[`02924a33ea56732fdce47d8a8673fb3c0752c425`](https://github.com/rivosinc/kvmtool/commit/02924a33ea56732fdce47d8a8673fb3c0752c425)
("riscv: cove: Don't emit interrupt_map for pci devices in fdt.").

That branch carries a `cov` file describing the patch series it contains, available at
https://github.com/rivosinc/kvmtool/blob/02924a33ea56732fdce47d8a8673fb3c0752c425/cov. It is useful for
understanding the intent and ordering of the CoVE changes.

This repository is a **source reference and aggregated porting** effort, not the result of a Git
cherry-pick or merge of the Rivos branch. The relevant changes were studied, then re-applied and
adjusted by hand on top of the upstream baseline, so commit boundaries and individual hunks do not
correspond one-to-one to the reference branch.

## XuanTie Downstream Changes

The changes in this tree adapt kvmtool to the XuanTie platform and to the CoVE DP-1 POC
implementation. They are meant to work together with the `xvm2` TSM implementation in this
repository and the CoVE-enabled Linux kernel distributed separately in the runtime Release
bundle.

The libfdt dependency is vendored under `third_party/libfdt/` so the build does not need an
external copy of the library. See `third_party/libfdt/README.md` for its origin, file list, and
licensing.

## Licensing

kvmtool as a whole is distributed under the GNU General Public License v2; the full text is in
`COPYING` in this directory.

Some individual files may carry different file-level terms, such as BSD-style licenses or a
syscall-note exception. Where a file header states its own terms, that statement governs that file.

The vendored libfdt copies are dual-licensed; their terms are documented in
`third_party/libfdt/README.md`.

## Notes

The limitations, dependencies, and run conditions recorded in Rivos' `cov` file are historical
references for that branch. The current feature set and run instructions are those in the
repository root `README.md`, which supersedes them.

The upstream kvmtool `README` in this directory is kept unchanged, so parts of it may not reflect
the CoVE-specific behavior of this tree.
