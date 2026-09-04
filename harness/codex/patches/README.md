# Local Upstream Patches

This directory contains minimal source-level compatibility patches required to
compile the locked Codex release. It is part of the harness source and must not
depend on a developer-machine path.

`codex-utils-pty` is copied from the locked `rust-v0.152.1` source tree. The
only source deltas are in `src/win/conpty.rs` and
`src/win/procthreadattr.rs`: explicit pointer casts required by the resolved
Windows API types. Its `Cargo.toml` is standalone only because Cargo path
patches cannot inherit the upstream workspace manifest values.

Before updating `upstream.lock`, compare this directory with the new upstream
crate. Drop the local override when the new release compiles without it; do not
carry it forward by default.
