# Agent Model Switch Contract

## Goal

Make model selection truthful for every Agent supported by iyw-claw. A model
must not be shown as applied until the running Agent reports that it accepts and
is using the requested value. Agents that do not expose a reliable model
selector must not show a misleading chat-level model control.

This change covers the 13 built-in identities and trusted Registry identities,
including Claude Code, Codex, OpenCode, Gemini, OpenClaw, Cline, Hermes,
CodeBuddy, Kimi Code, Pi, Grok, Cursor, and DeepSeek.

## Current failure

The Fusion request already accepts an Agent Platform database ID, but the local
Rust model catalog currently stores one global list. An Agent-scoped response is
merged into that list, so a model compatible with one Agent can be projected
into another Agent's native configuration. The provider overlays then write a
model list that is broader than the Agent-specific `/models?sdk_id=...`
contract.

The chat panel also starts its 45-second model-reapply timer when the user picks
a model. The timer includes time spent waiting for an active operation to end
and time before a replacement connection is created. Slow Agents, especially
Pi in the current runtime, can therefore be reported as failed even though the
replacement is still progressing.

Finally, not every ACP adapter exposes a model config option or accepts the
same model spelling. Treating all adapters as if they supported the same
interactive model protocol creates false failures such as:

> 当前 Agent 未确认该模型，已恢复原模型。

## Design

### 1. Agent-scoped catalog ownership

Extend the runtime catalog to keep two independent layers:

- the complete catalog, used only for global discovery and backward-compatible
  metadata;
- an Agent-scoped catalog keyed by the stable Agent Platform `platform_id`
  (the decimal database ID already used by `iyw_account_list_models`).

The local `registry_id` remains a directory/process key only. It must never be
used as the cross-service model compatibility key.

Each scoped entry records:

- whether the response is authoritative, including an authoritative empty
  response;
- the ordered model IDs and their capability/limit snapshots;
- the last successful payload version needed for persistence and diagnostics.

Persist the scoped layer in a new catalog schema version while preserving the
complete layer and reading the existing schema as a complete-only catalog. A
successful complete fetch replaces only the complete layer. A successful
Agent-scoped fetch replaces only that Agent's layer. A failed fetch keeps the
last successful layer; an Agent with no scoped cache must not silently use a
different Agent's scoped data.

`model_ids_for(agent)` and all provider overlay writers must resolve the
Agent's scoped layer first. Before that layer is authoritative they may use the
existing seed/complete compatibility path so startup remains backward
compatible. Once it is authoritative, an empty scoped layer is a deliberate
"no compatible model" result and must fail closed with a visible configuration
error rather than indexing an empty list or borrowing the complete catalog.

The frontend catalog cache follows the same rule: complete and per-Agent
payloads stay separate, and an authoritative empty per-Agent response produces
no model option.

### 2. Runtime capability classification

Do not hard-code a claim that every Agent supports the same switching protocol.
At connection initialization, classify the live session from its actual ACP
snapshot:

- **Interactive model switch**: the session advertises a selectable config
  option with model category or `id = "model"`, has at least one selectable
  value, and the adapter accepts `session/set_config_option`.
- **Startup-only model**: the Agent has a product-owned scoped catalog and a
  native provider projection, but the live ACP session does not expose a
  reliable model option or does not acknowledge a set request.
- **No managed model**: no authoritative compatible catalog exists, or the
  Agent is not routed through the managed provider overlay.

The classification is runtime evidence, not a second list of optimistic
adapter assumptions. It is kept in the connection snapshot for the frontend
and included in structured diagnostics without secrets or full payloads.

For interactive sessions, the model option shown in chat is the intersection
of the authoritative Agent-scoped catalog and the live ACP option. A value not
advertised by the live Agent is never sent as an ordinary set request. If the
user selects such a newly published scoped value, the guarded replacement path
is used only when the Agent has a startup projection and a reliable post-start
readback path. Otherwise the chat model control stays hidden and settings remain
the place to configure the startup model.

For startup-only sessions, a model preference is passed through the existing
preferred-config/native-overlay path on the next connection. Success is based
on the fresh process's reported current model when that report exists; it is
not inferred from a local write alone. If no readback exists, the UI does not
claim an interactive switch succeeded.

### 3. Model identity and acknowledgement

The canonical value sent to an Agent is the value from that Agent's live ACP
option. Adapters may resolve a user-facing alias to a canonical ID, but the
frontend must compare the returned `current_value` to the canonical value, not
to an arbitrary Fusion display name.

`session/set_config_option` remains awaited through the ACP command channel. The
command returns the Agent's updated option snapshot, and the reducer applies
that snapshot atomically. Optimistic localStorage state is treated as a desired
preference only; it is never acknowledgement.

### 4. Replacement lifecycle and timeout

The existing same-session replacement keeps the external session ID and
conversation history. Its state machine is tightened as follows:

1. Record the source connection ID and desired canonical model.
2. If the session is prompting or the backend reports an active operation,
   defer replacement without starting the model-switch watchdog.
3. After the old connection is actually replaced and a new connection ID is
   created, start the watchdog from that creation timestamp. Waiting for the
   old operation and retry backoff does not consume the Agent initialization
   budget.
4. Wait for `selectors_ready` and validate both conditions:
   - the target is present in the live model option; and
   - `current_value` equals the target canonical value.
5. On success, clear the attempt and persist the confirmed value. On an Agent
   rejection, missing option, connection error, or initialization timeout,
   restore the last Agent-reported current value and remove the desired value
   from the pending UI state. Never replay a failed prompt or silently bill a
   second turn.

Use a bounded per-Agent initialization budget derived from observed startup
stages, with a conservative default for unknown/trusted adapters. The budget
starts only after step 3 and is cancelled when the first valid selector
snapshot arrives. A failed replacement retry remains observable in structured
logs with Agent type, source/target connection IDs, phase, and reason.

### 5. Provider overlay boundaries

Keep the existing native file formats and atomic writes. Change only the model
source and failure behavior:

- Claude Code, Codex, OpenCode, OpenClaw, Cline, Hermes, CodeBuddy, Kimi Code,
  Pi, and Grok read their Agent-scoped model list.
- Gemini, Cursor, DeepSeek, and trusted adapters without a reviewed managed
  overlay do not receive an invented model projection.
- Empty authoritative lists fail closed with a typed error and actionable
  configuration state; they must not panic, write an arbitrary default, or
  fall back to another Agent's models.

### 6. Frontend state and display

Add the runtime classification and scoped-catalog authority to the existing
connection snapshot. The composer/model picker follows these rules:

- authoritative scoped catalog plus a live interactive option: show the
  intersection and permit the guarded switch;
- startup-only or no managed model: hide the chat-level model picker while
  leaving the Agent settings model fields available;
- authoritative empty catalog: show the existing configuration error state,
  not an empty selectable control;
- during a guarded replacement: keep the conversation usable and hide
  transient connecting/error task UI, as the existing silent-reconnect contract
  requires.

Do not change queue ownership, prompt generation, viewer attachment, or
delegation-child ownership. Viewers and delegation children never restart the
owner's Agent and never report a local model switch as confirmed.

## Data flow

```text
Fusion /v1/models?sdk_id=<platform_id>
        |
        v
scoped frontend cache + scoped Rust catalog
        |
        +--> native provider projection for the selected Agent
        |
        +--> fixed model catalog (authoritative only)
                    |
                    v
             live ACP configOptions
                    |
                    +--> interactive set + awaited acknowledgement
                    |
                    +--> guarded same-session replacement
                              |
                              v
                    selectors_ready + current_value validation
```

## Error handling and observability

- Preserve the last successful scoped catalog on network/transport failure.
- Treat authoritative empty as a typed unavailable-model state.
- Restore only the last Agent-reported model after a failed switch.
- Do not include tokens, endpoint URLs, full ACP payloads, or user prompts in
  diagnostics.
- Log the Agent type, stable platform ID when available, source/target
  connection IDs, replacement phase, elapsed initialization time, and typed
  outcome.

## Validation matrix

Static and focused validation must cover each behavior class:

| Class | Examples | Required evidence |
| --- | --- | --- |
| Interactive | Claude Code, Codex, any adapter exposing model config | canonical set acknowledgement; selector presence and current value |
| Slow interactive | Pi and any adapter with delayed selectors | watchdog starts after replacement creation; no premature rollback |
| Alias-resolving | Claude aliases or provider-specific model names | canonical value comparison and readback |
| Startup-only | adapters with native config but no reliable model option | picker hidden; next connection receives preference; no false success |
| Empty scoped catalog | valid Agent Platform ID with zero compatible models | no global fallback; typed unavailable state; no panic |
| Failed scoped fetch | offline/HTTP failure | previous scoped cache retained; no cross-Agent fallback |
| Viewer/delegation child | cross-client attach and child sessions | no owner restart; no local success toast |

Required repository gates follow `iyw-claw/AGENTS.md`: static call-chain review,
TypeScript type/lint/format checks for changed files, Rust formatting/checks
where available, and `git diff --check`. Desktop builds and test suites are
not run by default in this repository and will be reported separately if not
requested.

## Scope boundaries

This design does not change billing, prompt queue semantics, Agent installation,
Fusion routing, or the public `/v1/models` method. It changes only the local
catalog ownership, provider projection source, runtime model capability
classification, and model-switch confirmation lifecycle.
