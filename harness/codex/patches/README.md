# Local Upstream Patches

The command-purpose display extension and its upgrade checklist are documented
in [COMMAND_DESCRIPTION.md](COMMAND_DESCRIPTION.md).

`codex-app-server` also forwards the canonical command start when it carries a
process ID, even if an approval placeholder already used the same item ID.
The harness merges this update and retains the original thread/turn ownership
until command exit. A verified late exit updates only process activity, so a
background command can settle after its originating turn without entering a
new turn's transcript. Recheck this path, including approval placeholders, when
upgrading the upstream protocol.

`codex-mcp` retains the production sources of pinned 0.155.0 (`f0a1b8f`), with
test-only modules omitted and standalone dependency metadata. Its status inspection
reads one published runtime generation without starting/reconnecting clients.
Thread-scoped `mcpServerStatus/list` uses that view through `codex-core`; global
status requests retain upstream discovery. Resource lists still use ready clients,
and tool filters, schemas, startup errors, authentication and pagination remain intact.
Review this patch on upstream upgrades: status probes must not create a second
initialize request against the host's single-principal MCP session binding.

This directory contains minimal source-level compatibility patches required to
compile the locked Codex release. It is part of the harness source and must not
depend on a developer-machine path.

`codex-state` contains the production sources and migrations from pinned
0.155.0, with test-only items omitted. Migration SQL is stored with LF endings.
Before migration, the runtime accepts an applied checksum only when it matches
the exact embedded SQL or its LF/CRLF variant. It adjusts the in-memory migrator,
preserving database migration records, locking and rejection of other changes.
The `codex-core` state bridge also exposes the upstream fallible initializer so
the desktop worker reports initialization errors before accepting sessions;
it must not advertise a working runtime with no state DB. Review these patches
on upgrades and preserve all production dependencies and migrations.

`codex-core` treats HTTP 413 as a request-size rejection, not a reconnectable
stream failure. A sampling turn attempts its existing configured compaction
once before continuing; a repeated rejection or failed compaction surfaces the
original error. Local and remote-v2 compaction do not retry an unchanged 413
request. No history is truncated outside the upstream compaction lifecycle.

`codex-utils-pty` is copied from the locked `rust-v0.155.0` source tree. Local
source deltas retain explicit pointer casts in `src/win/conpty.rs` and
`src/win/procthreadattr.rs`, plus hidden-window creation flags in `src/pipe.rs`,
`src/win/mod.rs`, and `src/win/psuedocon.rs`. Its `Cargo.toml` is standalone
because Cargo path patches cannot inherit upstream workspace manifest values.

`src/win/job.rs` also retains `CREATE_NO_WINDOW` alongside `CREATE_SUSPENDED`
when starting internal Git and MCP subprocesses. Assigning the process to a job
must not undo the hidden-window setting.

`codex-shell-command` contains the production sources from the same locked
`rust-v0.155.0` commit. Its only runtime change is setting `CREATE_NO_WINDOW`
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

The 0.154.0 upgrade refreshes all vendored Codex production crates from commit
`6b9826e3aa83b1a5947db50f4332cb9c65f1b340`. Three-way source migration preserves
the Windows launch flags, renamed helper paths, command descriptions, and
legacy thread-history adapter. Production dependency changes and precomputed
protocol exports are synchronized; test-only source additions are omitted.
The unchanged PTY source still needs the existing pointer-cast patch.
The 0.155.0 upgrade refreshes production sources and dependencies from
`f0a1b8f0849d90960bc406b848f32e5a129b0457`. It preserves the local Windows,
command-description, MCP status, SQLite checksum and HTTP 413 patches through
three-way source migration. Both compressed protocol exports retain the optional
command description in JSON schemas and TypeScript. The new model-provider AWS
credential export subprocess also uses `CREATE_NO_WINDOW`. Upstream production
migrations are included; new test-only sources are omitted. The existing PTY
pointer casts are still required.

## MCP Tool Identity Diagnostics

The Codex 0.154.0 registry patch classifies a namespace-only invocation, a
missing namespace, and an unregistered identity before execution. It preserves
the official structured tool routing and does not add alias guessing. This
allows the host to distinguish a provider/relay namespace compatibility error
from an unavailable business capability. See `docs/runtime-repairs-20260911.md`
at the repository root for the observed production routing configuration.
