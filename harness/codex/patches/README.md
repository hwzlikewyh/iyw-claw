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
on the two PowerShell detection commands in `src/powershell.rs`. Detection calls
`pwsh` directly instead of adding an intermediate `cmd /C` process. The standalone
manifest resolves the original workspace dependencies at the same pin. Upstream
test modules, test-only PowerShell parser, and fixtures are omitted from this
runtime patch; no new tests are added. Review both overrides on each upgrade.

`codex-windows-sandbox` contains the pinned production library and its two
helper targets. Package lookup uses `xinghe-resources`,
`xinghe-command-runner.exe` and `xinghe-windows-sandbox-setup.exe` so the runtime
resolves the renamed bundle. Cargo target names and the setup manifest name
match those files. Existing sandbox account, service, config and protocol
identifiers are retained for compatibility. Hidden-window flags also cover
non-elevated setup refresh, the working-directory junction helper and the
read-deny ripgrep probe. Test modules, files and fixtures are omitted; the standalone manifest
keeps the upstream dependency versions and commit pin.

`codex-git-utils` contains the pinned production sources with hidden-window
flags on synchronous Git commands and on the asynchronous Job Object fallback.
The fallback clears `CREATE_SUSPENDED` while retaining `CREATE_NO_WINDOW`.
Timeouts, process-tree termination and Git arguments remain unchanged. Its
manifest resolves the original workspace dependencies at the same versions and
commit; upstream test modules and fixtures are omitted.

The `0.153.4` upgrade compared both patched Windows files and the upstream
crate manifest with `0.152.1`; they are unchanged. The pointer casts remain
necessary. The upgrade also adopts upstream's `AsRef<OsStr>` pipe-spawn API
while preserving the existing hidden-window flags, and updates the package version.

`codex-core-plugins` contains the pinned production sources. Plugin startup sync
creates its own synchronous Git commands, bypassing `codex-git-utils`.
`PluginGitMode::command` now sets `CREATE_NO_WINDOW` on Windows; marketplace
installation uses the same factory in Manual mode with unchanged Git arguments
and environment. This covers remote HEAD lookup, local rev-parse, clone, fetch,
checkout and submodule operations without changing plugin configuration.
The separate npm package materialization command receives the same Windows flag.
Upstream test modules and fixtures are omitted; dependencies retain the pinned
workspace versions. Review this override together with `codex-git-utils` on upgrade.

The Windows process audit also requires pinned production copies of `codex-core`,
`codex-exec-server`, `codex-hooks`, `codex-rmcp-client`, `codex-rollout`,
`codex-login`, `codex-model-provider`, and `codex-code-mode`. Their runtime deltas
set `CREATE_NO_WINDOW` at background command creation, hook Job fallback and
taskkill cleanup. The original arguments, environment, timeout, cancellation and
sandbox policy are retained. Standalone manifests preserve every production
dependency's version, features, optional flag, target and default-feature setting.
Bundled production assets are copied byte-for-byte. Test modules and fixtures
are omitted; small routing/instruction helpers still referenced by library
constructors retain their upstream module aliases. The upstream internal sync
handler remains compiled under its original identifiers using `internal_sync*`
source files, without its test module.

All three private Windows helper targets use the Windows GUI subsystem with
inherited pipes. Bundle verification rejects console-subsystem helpers. This
also covers helper launches outside the patched factories. See
`WINDOWS_PROCESS_AUDIT.md` for call paths, exclusions and verification limits.

The sandbox's three non-PTY execution paths explicitly use
`ConsoleMode::NoWindow`: legacy captured execution, legacy unified execution,
and the elevated command runner. Their parent is a GUI process with no console
to inherit; pipes must not cause Windows to allocate a new visible console.
PTY branches, restricted tokens, desktops and pipe handling are unchanged.

`aws-config` copies production sources and the Apache license from locked
crates.io version 1.11.0. Its sole runtime delta sets `CREATE_NO_WINDOW` on the
Windows `credential_process` shell. The archive SHA-256 is
`a767267da9e2c2e189b2f9df8b5657e850ecf5352644734ba130d4a57095cf1b`.
All 29 production dependency declarations and all feature definitions match the
published manifest; test items and fixtures are omitted. This is independent of
the upstream model provider's explicit AWS refresh command.

`appcontainer-common` copies the production sources and MIT license of
Microsoft's `appcontainer_common` 0.8.0 at
`6cd3d58f05d3447e67109cfb75e042803b843ca4`. On Windows x86,
`SHELLEXECUTEINFOW` is packed: calling a method directly on `hProcess` creates
an unaligned reference. `proxy_coordinator` copies that field to a local value
before checking it and transferring it to the existing owned-handle wrapper.
All dependency and feature declarations are retained; test modules and their
orphaned documentation are omitted. Review this override when updating MXC.

Before updating `upstream.lock`, compare this directory with the new upstream
crate. Drop the local override when the new release compiles without it; do not
carry it forward by default.
