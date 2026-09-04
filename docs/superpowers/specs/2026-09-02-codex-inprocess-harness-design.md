# Codex In-Process Harness And Same-Executable Worker Design

## Status

- Design date: 2026-09-02
- Decision: use the locked Codex App Server in-process client behind an
  iyw-claw-owned adapter, hosted by a private worker child started from the
  same `iyw-claw.exe` image.
- Repository boundary: implementation starts in
  `harness/codex` on an isolated branch/worktree.
- Default behavior: existing external ACP remains enabled and is the fallback.
- Release rule: the in-process path cannot become the default until every
  required capability in the parity matrix has passing regression evidence.

## Objective

Keep one user-facing `iyw-claw.exe`. For an opted-in Codex session, that
executable starts itself with an internal worker flag; the child loads a private
Codex worker DLL and runs the App Server in-process. The parent keeps iyw-claw's
session, queue, permission, storage, and UI contracts authoritative. This
removes the Codex CLI/TUI front doors without publishing a second worker
executable or creating a second application session system.

Success requires all of the following:

1. A new Codex connection can start, stream a turn, steer it, cancel it, and
   shut down without duplicate prompts or unresolved requests.
2. New and resumed conversations preserve the same conversation identity,
   turn generation, queue ordering, attachments, goals, and recovery behavior
   as external ACP.
3. Filesystem, terminal, command, MCP, image, Skill, and permission actions
   remain scoped to the owning iyw-claw connection and can be revoked.
4. Native Codex threads, turns, server requests, and events cannot affect a
   replaced or unrelated iyw-claw session.
5. Windows, macOS, and Linux release artifacts prove the same behavior before
   the feature is enabled by default.

## Evidence Baseline

The current iyw-claw implementation has these ownership points:

- `src-tauri/src/acp/manager.rs` owns `ConnectionManager`, prompt
  serialization, durable input outbox, conversation binding, generation,
  delegation, and teardown.
- `src-tauri/src/acp/connection.rs` owns the ACP session loop, new/resume
  lifecycle, event projection, permission admission, image validation, and
  shutdown settlement.
- `src-tauri/src/acp/runtime_host.rs` and
  `src-tauri/src/acp/runtime_host_router.rs` share a Codex host and route every
  session through a generation-checked `RuntimeSessionRoute`.
- `src-tauri/src/acp/runtime_host_requests.rs` owns host-side filesystem,
  terminal, permission, and capability-revocation enforcement for ACP.
- `src-tauri/src/commands/acp.rs` provides the single
  `build_session_runtime_env` path, Skill reconcile, provider overlay,
  credentials, private `CODEX_HOME`, and model catalog setup.
- `SessionState`, `AcpEvent`, SQLite, and the outbox remain the application
  source of truth. Codex's own thread store is an execution/runtime source,
  not a replacement for iyw-claw conversation state.

The locked upstream is `openai/codex` tag `rust-v0.152.1`, peeled commit
`5adb68a49933ae446bf11935662c83dba55a0804`, with the tag object and Cargo
patches recorded in `harness/codex/upstream.lock`.

`codex-app-server-client` has no `codex-cli`, `codex-tui`, or `ratatui` package
in its resolved graph. It still depends on `codex-app-server`, `codex-core`,
`codex-exec-server`, sandboxing, PTY, MCP, Skills, image, network, and many
extension crates. Therefore package reduction and malware/AV behavior require
release artifact measurements; they cannot be inferred from removing TUI code.

## Architecture

```text
ConnectionManager / AgentInputRuntime / SQLite outbox
                         |
          same-executable ACP worker child
          (iyw-claw.exe --internal-codex-worker)
                         |
                 private worker DLL
                         |
          CodexHarness + InProcessAppServerClient
                         |
                   Codex App Server core
```

`CodexHarness` is hosted by the private worker child. It is an alternate Agent
backend, not a second connection manager. The parent process owns the ACP
connection and application state; the worker owns only:

- a worker-local in-process runtime keyed by an explicit effective-config
  fingerprint;
- a protocol-neutral session/thread binding table;
- typed request/event translation;
- server-request admission and response routing;
- capability and generation checks at the boundary.

It does not own durable prompts, conversations, UI snapshots, Skill inventory,
account credentials, or a second outbox. Upstream types remain private to the
adapter module and never cross into `src-tauri/src/acp` or frontend DTOs.

The parent `iyw-codex-harness` crate remains dependency-free by default. The
upstream client is linked only into the private `iyw-codex-worker` `cdylib`, so
ordinary iyw-claw checks do not fetch or link the 1,000+ crate Codex graph.

## Lifecycle And Data Flow

### Runtime startup

1. `ConnectionManager` obtains the normal runtime configuration from
   `build_session_runtime_env` and the active private Agent storage profile.
2. For `IYW_CLAW_CODEX_BACKEND=internal-worker`, the parent validates the
   private DLL beside the application or in its packaged resource directory and
   starts its own executable with `--internal-codex-worker`. Other values keep
   the existing external ACP path.
3. The worker converts the inherited, validated context into an explicit Codex `Config`,
   `EnvironmentManager`, `CloudConfigBundleLoader`, feedback/log/state handles,
   client identity, and bounded channel capacity. No process-wide environment
   mutation is used for per-session configuration.
4. The worker-local runtime is keyed by the same effective configuration values that
   determine Codex host reuse. A different provider, profile, Skill generation,
   sandbox policy, or credential generation cannot silently reuse an old host.
5. `InProcessAppServerClient::start` performs the App Server initialize
   handshake. A failure leaves no published route and falls back to external
   ACP before the worker route is published. The worker backend never shares
   Runtime Host or built-in MCP authority with another connection.

### New and resumed sessions

1. The application creates or loads its SQLite conversation and assigns a
   connection generation before sending any Codex request.
2. New sessions use `thread/start`; resumed sessions use `thread/resume` with
   a validated stored thread id and the expected private Codex home.
3. The returned thread id is bound to `(connection_id, conversation_id,
   generation, runtime_fingerprint)` before events are accepted.
4. A thread id from another connection, an old generation, or an unexpected
   Codex home is rejected as stale. It is never forwarded to the UI or queue.

### Prompt, steer, cancel

- A durable outbox claim creates the application turn record first, then sends
  `turn/start` with validated `UserInput` blocks.
- `turn/steer` always includes the current active turn id as
  `expectedTurnId`. A stale steer is settled as rejected without creating a
  second turn.
- `turn/interrupt` is sent only for the bound thread and active turn. The
  application settles cancellation once, even if both a completion event and
  a transport close arrive.
- `turn/completed`, item events, and overload errors are translated into the
  existing `AcpEvent`/`SessionState` projection. The upstream response is not
  independently persisted as a second transcript.

## Capability Parity Matrix

| Capability | Upstream surface | Adapter contract | Required evidence before enablement |
| --- | --- | --- | --- |
| Initialize | internal initialize + initialized | one handshake per shared runtime | handshake, duplicate-init rejection, teardown |
| Session | `thread/start`, `thread/resume`, `thread/fork` | bind thread id to connection/conversation/generation | new, resume, stale and fork identity tests |
| Prompt | `turn/start` | durable outbox claim before send | ordering, retry, duplicate prevention |
| Steer | `turn/steer` | expected active turn precondition | mid-turn inject and stale steer tests |
| Cancel | `turn/interrupt` | one settlement and cleanup | interrupt during model/tool/approval wait |
| MCP | config plus `mcpServer/*` methods/events | snapshot iyw-claw MCP authority; never trust arbitrary server requests | built-in lease, user server, revoke, OAuth/elicitation policy |
| Skills | `skills/list`, `skills/extraRoots/set`, `skills/config/write`, changed events | reuse pre-launch Skill reconcile and private `CODEX_HOME` | inventory, enable/disable, changed notification, read-only built-ins |
| Permissions | command/file/permissions server requests | map to existing permission queue, resolve/reject by request id | allow once/always, reject, queue, teardown, revoke |
| Filesystem | Codex core environment and sandbox | explicit cwd and workspace roots; no ACP file callback assumption | path escape, read-only, write, root replacement, revoke |
| Terminal | Codex command execution and terminal items | explicit `EnvironmentManager`/sandbox; project terminal UI projection | long process, output, kill, cleanup, revoke |
| Images | native `UserInput` and image items | reuse validated attachment admission and local-file policy | image-only, mixed prompt, invalid path, output projection |
| Goals | `thread/goal/set|get|clear` and notifications | map to existing goal event/card identity | set, update, clear, resume, concurrent turns |
| Subagents | collaboration items, fork, child threads | parent-owned child bindings and existing delegation authority | parent/child identity, cancellation, depth, settlement |
| Queue | native thread queue methods | SQLite outbox remains authoritative; native queue is not dual-written | busy send, restart, replay, reorder, no duplicate billing |
| Auth/config | explicit start args, account/config methods | reuse credential projection and config reconciler | login state, provider overlay, no secret logging |
| Recovery | `thread/resume`, runtime shutdown/restart | existing recovery budget and generation fence | process/runtime crash, resume, fallback, partial output |
| Backpressure | bounded upstream channels, overload `-32001` | preserve queue item and return retryable state | full request/event queues, no dropped approval |
| Shutdown | client `shutdown()` with bounded drain | cancel pending work, revoke routes, await bounded close | normal close, timeout, repeated close |

## Security And Isolation

In-process execution removes a process boundary. That reduces launcher/shim
surfaces and may reduce AV false positives, but it increases the impact of a
memory-safety or logic failure inside the embedded Codex runtime. The harness
must therefore strengthen policy boundaries rather than assume embedding is a
security improvement.

- Never pass raw user-supplied paths, commands, URLs, headers, or credentials
  into a shell or unvalidated Codex config.
- Construct `Config` and `EnvironmentManager` explicitly. `CODEX_HOME` is a
  private, immutable profile path selected by Agent storage.
- Treat `cwd`, `runtime_workspace_roots`, `sandbox`, and `permissions` as a
  single capability set. Reject a request that expands any one dimension after
  a thread is bound unless the application explicitly advances its generation.
- Server requests are accepted only when their request id, thread id, turn id,
  connection id, and generation match a live binding. Unknown ids are rejected
  immediately, never parked indefinitely.
- Capability revocation interrupts active turns and terminates owned runtime
  work. It must not rely on ACP-only `ReadTextFile` or terminal callbacks,
  because in-process Codex core executes those operations internally.
- MCP leases, delegation tokens, questions, confirmations, and terminal trees
  are revoked during the same connection teardown path used by external ACP.
- Logs contain stable ids, phase, duration, and error category only. They never
  contain secrets, full prompts, signed URLs, cookies, or complete payloads.

## Error And Fallback Rules

The adapter exposes stable protocol-neutral errors: `NotReady`, `Overloaded`,
`StaleBinding`, `PermissionDenied`, `Unsupported`, `TransportClosed`,
`UpstreamFailed`, and `RecoveryRequired`.

- A startup/config/credential error prevents route publication and uses external
  ACP if it is available.
- An overload error preserves the original outbox item and is retryable only by
  the existing explicit queue policy. It never automatically replays a billable
  prompt.
- A stale binding rejects the event/request and records a bounded diagnostic;
  it never retries against a new thread.
- An unsupported experimental method fails explicitly. It cannot silently open
  a second runtime or fall back mid-turn.
- A runtime crash preserves partial assistant output and settles the active
  turn once. Recovery creates a new generation before attempting resume.

## Testing And Release Gates

The harness adds focused tests for protocol-neutral ownership and lifecycle
contracts first. Upstream integration tests run only in the isolated probe or
the harness's feature-enabled target.

Required gates before default enablement:

1. Compile the locked upstream graph on every supported target with the pinned
   toolchain or a documented compatibility patch. Windows `codex-utils-pty`
   and cross-target C toolchains are explicit prerequisites.
2. Run deterministic fake-App-Server tests for every row in the parity matrix,
   including malformed responses, queue full, duplicate ids, stale ids,
   approval timeout, cancellation, and shutdown.
3. Run existing iyw-claw ACP/session/outbox/recovery tests against both the
   external and in-process backends using the same fixtures.
4. Compare release binary size, dependency closure, startup latency, memory,
   and crash cleanup against the current external ACP build.
5. Run installed-package smoke on Windows x64, macOS x64/ARM64, Linux x64/
   ARM64: start, new thread, prompt, tool/permission round trip, image input,
   steer, cancel, resume, subagent/goal where enabled, close, and restart.
6. Run security checks for path escape, capability revocation, cross-session
   request injection, secret/log leakage, and child-process ownership.

Until all gates pass, external ACP remains the default application backend and
the internal worker remains opt-in. No release workflow or Fusion publication is
part of this design.

## Rollout And Rollback

After dependency isolation is approved, the first application implementation
adds a backend selector behind a disabled feature flag. It publishes health and
parity diagnostics but does not change the existing Codex registry distribution
or installer. A per-session opt-in can exercise the same-executable worker while
external ACP remains available.

Promotion requires a versioned feature flag and an explicit last-known-good
backend. On any startup, permission, recovery, or shutdown regression, new
sessions stop selecting in-process; existing sessions finish or recover through
the external ACP path after a generation fence. Rollback never deletes the
private Codex profile, SQLite data, outbox items, Skills, or user settings.

## Implementation Checkpoint (2026-09-03)

The isolated branch has a verified harness foundation only:

- `harness/codex` owns explicit method allowlists, session/thread/generation
  bindings, active-turn validation, request-id single settlement, and a
  fail-closed server-request admission boundary.
- Session-scoped command, file-change, user-input, permission, dynamic-tool,
  and MCP elicitation requests require their bound `threadId`; turn-scoped
  forms also require the live `turnId`. `turn/steer` requires its exact
  `expectedTurnId`.
- The facade rejects cross-session prompts, duplicate session ownership,
  malformed text blocks, and non-text delta payloads. It drops notifications
  without a thread id and stale turn completions; post-completion cancellation
  is idempotent.
- Initialize advertises image input only when `Images` is granted and session
  loading only for a validated persisted session id. Prompt conversion enforces
  the same image capability before constructing the upstream request.
- The upstream-enabled harness deliberately excludes Codex CLI/TUI entry
  points. Its helper dispatcher contains only upstream-required arg0 exec,
  filesystem sandbox, Windows sandbox, and apply-patch callbacks.

The application now has an opt-in `internal-worker` selector in
`src-tauri/src/acp/connection.rs`. The parent starts the same executable with
`--internal-codex-worker`; `src-tauri/src/internal_codex_worker.rs` loads only
the private resource DLL and dispatches the worker or its upstream helper modes
before Tauri initialization. The worker uses a dedicated Runtime Host with
deny-all host policy and receives no built-in MCP entries.

The worker graph is deliberately kept out of `src-tauri`: Cargo resolves the
upstream graph in `harness/codex-worker` only. `src-tauri` links the small
`libloading` loader, so ordinary application metadata remains free of the
Codex/SQLite native graph.

The direct integration is therefore paused at a dependency boundary: Codex
uses `sqlx 0.9` with `libsqlite3-sys 0.37`, while iyw-claw's SeaORM path uses
`sqlx 0.8.6` with `libsqlite3-sys 0.30.1`. Cargo rejects both native crates
because they share `links = "sqlite3"`. The upstream graph also requires
Codex's patched `tokio-tungstenite` fork. A local SQLite compatibility facade
compiled in isolated probes, but would change the application-wide native
SQLite binding and is not part of this experiment.

Keeping the graph in `harness/codex-worker` restores normal `src-tauri`
dependency resolution while preserving the independent ownership/lifecycle and
upstream compile evidence. `src-tauri/scripts/prepare-codex-worker.mjs` and
`tauri.codex-worker.conf.json` are experimental-only packaging steps; release
workflows do not invoke them. The same-executable worker is not a proven
installed-client or end-to-end feature until the release gates above pass.

Verification on Windows: the dependency-free harness test target passed 10/10
tests; the locked `upstream-acp` target passed 28/28 tests. The latter spent
approximately 55 minutes in a full debug build/link of the isolated Codex
graph, reinforcing that it must not be added to the ordinary application build.

## Runtime Validation Checkpoint (2026-09-03)

The private Windows x64 worker `cdylib` was built successfully from the locked
upstream graph with `cargo build --release --locked --target
x86_64-pc-windows-msvc`. The resulting DLL was 244,667,392 bytes and exported
both `iyw_codex_worker_run_v1` and `iyw_codex_worker_dispatch_helper_v1`.

A direct DLL protocol probe completed `initialize`, rejected a mismatched cwd,
then completed `session/new` with the owning cwd and returned a valid ACP
session id. A subsequent `session/prompt` reached Codex and returned
`stopReason=refusal` in the unauthenticated probe environment; no bridge or
JSON-RPC mapping error occurred. This proves worker loading and basic ACP
lifecycle, not authenticated model generation or installed desktop acceptance.

## Application Boundary Validation Follow-up (2026-09-04)

The application-side boundary was also built and exercised on Windows x64.
`cargo check --locked --manifest-path src-tauri/Cargo.toml --bin iyw-claw`
and `cargo build --locked --manifest-path src-tauri/Cargo.toml --bin iyw-claw`
both passed. After the experimental preparation script staged the private DLL,
the resulting debug `iyw-claw.exe --internal-codex-worker` returned ACP
`initialize` with protocol version 1 and completed `session/new` with a valid
session id using a fresh, temporary Codex profile and cwd. The probe did not
initialize Tauri or create a desktop window.

This confirms the same-executable early-dispatch, private DLL loading, and
basic ACP session path. It does not prove an installed-package resource layout,
authenticated model generation, or parity for MCP, Skills, permissions,
filesystem, terminal, recovery, and cross-platform packaging. The temporary
staged DLL, empty frontend resource directory, placeholder sidecar, and probe
profiles are removed after this verification.
