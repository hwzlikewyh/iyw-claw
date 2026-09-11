# Runtime Repair Integration

This change updates the desktop Xinghe runtime to Codex `rust-v0.154.0`
(`6b9826e3aa83b1a5947db50f4332cb9c65f1b340`). It preserves main's current
worker layout, native commands, automatic-turn ownership, legacy history
recovery, command descriptions, and Windows process fixes. The vendored
production crates and protocol exports are refreshed at the same pin.

## Resulting Behavior

- Closing an ACP host ends its main SACP connection without always waiting
  for forced timeout cleanup. Initialization errors retain bounded diagnostics.
- Failed Codex turns return their redacted cause through the existing error
  path. Failed turns settle the conversation status.
- Session MCP configuration remains required. Startup verifies the current
  thread's MCP connection and tool catalog, and logs the advertised tool names.
  The built-in gateway must retain its search/read/invoke schemas; third-party
  servers that only provide resources may legitimately have an empty catalog.
- A session's first MCP namespace is persisted in existing app metadata and
  reused on recovery. Legacy Codex tool calls provide the previous prefix when
  available, including the separate Responses `namespace` field and old flat
  tool names. Tokens and transport endpoints may change without renaming tools.
- Missing tool calls distinguish `tool_namespace_not_callable`,
  `tool_namespace_missing`, and `tool_not_registered`. They remain rejected
  before execution, with no speculative name repair or replay.
- `/goal status`, `pause`, `resume`, `clear`, and objective updates use native
  goal APIs. Cancellation pauses an active goal before stopping owned turns.
- Existing native thread names are restored. Unnamed conversations follow
  Codex's isolated temporary title-generation flow and persist the resulting
  thread name. Manual titles retain precedence; no separate Chat Completions
  summary is scheduled.
- WeCom 1.1.0 uses `auth init`, `chat groups list`, `chat messages list`, and
  `message send`. Its group discovery API returns groups only. Messages retain
  opaque provider IDs and use the current `msg_type` and `user_name` fields.
- OpenCLI readiness follows the doctor connectivity report rather than exit
  code alone. Only initial preflight can fall back to an enabled built-in browser;
  existing page actions are never replayed across providers.
- The live status row uses the current agent's existing avatar with a compact
  activity badge. Waiting and attention states retain their distinct colors.

## Namespace Compatibility Finding

The reported `mcp__iyw_claw_builtin_b77227276b407e7` is a namespace, not a
concrete gateway function. Codex emits MCP tools inside `namespace.tools`;
the namespace itself has no parameter schema. A provider that interprets the
outer group as a function can expose exactly the reported empty-schema tool.
The error occurs in Codex's registry before the host gateway is called.

This affects all MCP and other namespaced function tools on that route, not
only audio. Ordinary function tools keep their own schemas. The Fusion relay
already has deterministic namespace flattening and reverse mapping for tool
choice, history, streaming, and non-streaming responses. Its flattening branch
explicitly rejects unsupported namespaced custom tools rather than inventing
an empty schema. Codex uses direct exposure when the model does not support
deferred tool search.

A read-only production query on 2026-09-11 found both enabled Flash abilities
still configured as `native`: `350230990200975360` (`deepseek-v4-flash`) and
`349135985254899712` (`deepseek-v4-flash-vision-exp`). This matches the reported
failure pattern; the exact failing request's complete upstream trace remains
unavailable. Changing those abilities to `flatten` and publishing the runtime
snapshot is a separate production configuration action pending confirmation.
The client source changes alone do not establish that this route is repaired.

## Validation

The integrated tree is checked with desktop and server `cargo check`, the
Xinghe worker build, TypeScript checking, targeted ESLint, upstream tag/commit
verification, structured manifest and protocol-export validation, targeted
formatting, and `git diff --check`.
WeCom request shapes were checked with the pinned official CLI's local dry run.
Fusion's existing namespace regression checks passed separately using
`go test ./internal/application/relay -run Namespace -count=1`; no Fusion source
or test files were changed.

Repository policy excludes default automated tests. No paid model call, live
WeCom message, browser interaction, package installation, or production rollout
is performed by these checks. A rebuilt package still needs live-session
validation. The original development worktree's unrelated edits are retained.
