# Codex Harness

`iyw-codex-harness` is the integration boundary for the optional in-process
Codex runtime. It is intentionally separate from `src-tauri/src/acp` so the
existing external ACP path remains the default while the new adapter is built
and validated.

## Upstream boundary

The initial upstream pin is recorded in [`upstream.lock`](upstream.lock):

- `codex-rs/app-server`
- `codex-rs/app-server-client`
- `codex-rs/app-server-protocol`

The lock records both the annotated tag object and its peeled source commit.
Cargo dependencies use the peeled commit; synchronization verifies both values
so a rewritten release tag cannot silently change the compiled source.

The upstream `in_process` API provides bounded request/event queues and
graceful shutdown. The harness will translate that protocol into the existing
ACP-facing behavior. Upstream types must not be re-exported from application
modules.

## Current status

The parent crate is dependency-free by default. Its optional `upstream-acp`
feature and the separate `upstream-probe` compile the locked Codex graph only
when explicitly requested from this directory. `src-tauri` deliberately has no
dependency or feature for this crate yet, so the existing npm/runtime-seed ACP
implementation remains the only application backend.

The facade's command-execution and file-change permission mapping is harness
code only. It accepts ordinary allow-once, allow-for-session, and reject
decisions; policy amendments and unsupported responses are rejected rather
than implicitly allowed. It is not wired to the application permission queue.

The ACP initialize response advertises image input only when the runtime grants
`Images`, and session loading only for a bridge with a validated persisted
session id. The prompt mapper enforces the same image capability, so a client
cannot bypass capability negotiation by sending an image block directly.

## Existing feature compatibility

The adapter must preserve the following ownership boundaries before it can
replace the current path:

| Capability | Codex App Server | Harness responsibility |
| --- | --- | --- |
| ACP initialize/session | Not ACP; uses `thread/*` | Translate requests, IDs and lifecycle events |
| Prompt/steer/cancel | `turn/start`, `turn/steer`, `turn/interrupt` | Preserve turn-generation and cancellation settlement |
| MCP | `thread/start`/`thread/resume` accept MCP configuration | Reuse iyw-claw URL, command, header and ownership validation |
| Skills | `skills/list`, extra roots and changed notifications | Keep `CODEX_HOME`, project roots and shared-skill publication consistent |
| Permissions/files/terminal | App Server events and server requests | Route through existing permission and filesystem policies |
| Images | Native input/output items | Reuse managed attachment validation and image event projection |
| Goals/subagents | Native thread goal and collaboration APIs | Map IDs, parent ownership and existing UI events |
| Persistence/queue | Codex thread store and queue APIs | Keep iyw-claw SQLite/outbox as the application source of truth |
| Auth/config | Codex config and account APIs | Reuse existing credential/config projection without duplicate writers |

“Supported upstream” is not the same as “supported by iyw-claw”. A row is only
complete after its bridge mapping and regression evidence exist. Unsupported or
experimental upstream methods must fail explicitly or stay behind the harness
feature gate; they must not silently fall back to a second live runtime.

## iyw-claw integration checkpoints

The existing `ConnectionManager` owns prompt serialization, durable input
outbox workers, conversation binding, turn generations, attach/viewer
semantics, and connection teardown. `connection.rs` owns the ACP session loop,
while `runtime_host.rs` and `runtime_host_registry.rs` share a Codex ACP host
by configuration fingerprint and route each logical session through a
`RuntimeSessionRoute`.

The harness must therefore be an alternate ACP agent endpoint, not a second
connection manager. It must preserve these existing sources of truth:

- `build_session_runtime_env` is the single launch configuration path,
  including provider overlay, credentials, `CODEX_HOME`, and Skill reconcile.
- `AgentStoragePaths::profile(AgentType::Codex)` owns the private Codex home.
- `RuntimeSessionRoute` owns per-session permission, filesystem, terminal and
  built-in MCP authority.
- `AgentInputRuntime` and the SQLite outbox own queued/steered input state.
- `SessionState` and `AcpEvent` own UI snapshots, replay, recovery and usage
  projection.

An in-process runtime cannot use per-process environment mutation as a session
configuration mechanism. The harness must construct Codex config explicitly,
key a shared runtime by the same effective configuration fingerprint, and
reject stale or cross-session server requests before forwarding them.

The crate now exposes protocol-neutral `SessionOwnership`, `SessionBinding`,
`RequestClass`, and `Capability` contracts. These are the first validation
boundary for the eventual bridge: a Codex `threadId` is not trusted until it is
bound to the application connection, conversation and generation that created
it. Removing a binding also requires the same owner and generation, which
prevents a late response from a replaced runtime affecting a new session.

The separate `upstream-probe` package provides an explicit compile boundary
around OpenAI's `codex-app-server-client` at the locked commit. It is not part
of the default build and its upstream types are not exported from application
modules. The feature-enabled harness has only a narrow child-helper dispatcher
for App Server's sandbox/filesystem/apply-patch callbacks; it does not include
Codex CLI or TUI startup.

The ACP facade also fences one session per bridge connection, requires prompt
and notification thread identity, drops stale turn completions, and settles
cancellation idempotently. These are lifecycle guards only; they do not imply
permission, MCP, terminal, Skill, or queue parity.

## Synchronization

Run `scripts/sync-upstream.ps1` to verify the locked release tag still points
at the recorded commit. Updating the pin is a separate change and must include
protocol, lifecycle, permission, MCP, image, subagent, backpressure and
shutdown verification before proposing application integration.

Do not commit generated binaries or a moving `main` reference. Keep any local
compatibility changes in `patches/` and document why each patch is required.
The locked Windows build currently uses `patches/codex-utils-pty`; compare it
with the next upstream release and remove it when the upstream crate compiles
without the two pointer-cast fixes.

## License

Codex is Apache-2.0. Preserve the upstream `LICENSE`, `NOTICE`, and applicable
third-party notices when source or binary artifacts are distributed.
