# Local Upstream Patches

This directory contains minimal source-level compatibility patches required to
compile the locked Codex release. It is part of the harness source and must not
depend on a developer-machine path.

`codex-utils-pty` is copied from the locked `rust-v0.153.4` source tree. Local
source deltas retain explicit pointer casts in `src/win/conpty.rs` and
`src/win/procthreadattr.rs`, plus hidden-window creation flags in `src/pipe.rs`,
`src/win/mod.rs`, and `src/win/psuedocon.rs`. Its `Cargo.toml` is standalone
because Cargo path patches cannot inherit upstream workspace manifest values.

The `0.153.4` upgrade compared both patched Windows files and the upstream
crate manifest with `0.152.1`; they are unchanged. The pointer casts remain
necessary. The upgrade also adopts upstream's `AsRef<OsStr>` pipe-spawn API
while preserving the existing hidden-window flags, and updates the package version.

Before updating `upstream.lock`, compare this directory with the new upstream
crate. Drop the local override when the new release compiles without it; do not
carry it forward by default.
