# Local Upstream Patches

This directory contains minimal source-level compatibility patches required to
compile the locked Codex release. It is part of the harness source and must not
depend on a developer-machine path.

`codex-utils-pty` is copied from the locked `rust-v0.153.4` source tree. Local
source deltas retain explicit pointer casts in `src/win/conpty.rs` and
`src/win/procthreadattr.rs`, plus hidden-window creation flags in `src/pipe.rs`,
`src/win/mod.rs`, and `src/win/psuedocon.rs`. Its `Cargo.toml` is standalone
because Cargo path patches cannot inherit upstream workspace manifest values.

`src/win/job.rs` also retains `CREATE_NO_WINDOW` alongside `CREATE_SUSPENDED`
when starting internal Git and MCP subprocesses. Assigning the process to a job
must not undo the hidden-window setting.

`codex-shell-command` contains the production sources from the same locked
`rust-v0.153.4` commit. Its only runtime change is setting `CREATE_NO_WINDOW`
on the two PowerShell detection commands in `src/powershell.rs`. The standalone
manifest resolves the original workspace dependencies at the same pin. Upstream
test modules, test-only PowerShell parser, and fixtures are omitted from this
runtime patch; no new tests are added. Review both overrides on each upgrade.

`codex-windows-sandbox` contains the pinned production library and its two
helper targets. Package lookup uses `xinghe-resources`,
`xinghe-command-runner.exe` and `xinghe-windows-sandbox-setup.exe` so the runtime
resolves the renamed bundle. Cargo target names and the setup manifest name
match those files. Existing sandbox account, service, config and protocol
identifiers are retained for compatibility. Runtime behavior otherwise matches
upstream. Test modules, files and fixtures are omitted; the standalone manifest
keeps the upstream dependency versions and commit pin.

The `0.153.4` upgrade compared both patched Windows files and the upstream
crate manifest with `0.152.1`; they are unchanged. The pointer casts remain
necessary. The upgrade also adopts upstream's `AsRef<OsStr>` pipe-spawn API
while preserving the existing hidden-window flags, and updates the package version.

Before updating `upstream.lock`, compare this directory with the new upstream
crate. Drop the local override when the new release compiles without it; do not
carry it forward by default.
