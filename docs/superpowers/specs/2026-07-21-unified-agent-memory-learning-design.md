# Unified Agent Memory and Conservative Learning Design

## Status

- Date: 2026-07-21
- Scope: Phase 2 of the existing user-memory feature
- Code baseline: `v0.1.9` (`a9ce94a`)
- Reference reviewed: `self-improving-1.2.16`

This design keeps the Phase 1 host-owned memory architecture and extends it
with a strict desktop storage root, observable Agent capabilities, and a
conservative candidate-review flow. It does not replace the existing three
canonical documents or create private memory stores for individual Agents.

## Outcome

Every supported Agent reads the same enabled User Memory, User Profile, and
User Soul context. Agents with a safe MCP transport and healthy required
resources can update confirmed durable memory, submit a conservative learning
candidate, or both. Each capability is evaluated independently. Agents without
a safe MCP transport remain read-only and use the existing host UI as the
manual fallback.

On desktop, active memory content is stored under the current user's
`~/.iyw-claw` directory by default, for example:

```text
C:\Users\Administrator\.iyw-claw\user-memory.md
C:\Users\Administrator\.iyw-claw\user-profile.md
C:\Users\Administrator\.iyw-claw\user-soul.md
```

An installed or portable desktop may continue to redirect databases, Agent
runtimes, uploads, and other application data through `IYW_CLAW_HOME`, but it
must not silently redirect user memory away from the user's home directory.

## Scope

Phase 2 includes:

1. A dedicated user-memory path override and deterministic desktop migration.
2. A shared three-part capability model for all eleven Agent types.
3. Runtime visibility when the MCP companion is missing or incompatible.
4. A backwards-compatible confirmed-memory append flow.
5. A new conservative candidate flow for reusable corrections and uncertain
   preferences.
6. Host-side candidate review, confirmation, rejection, and audit metadata.
7. Backup, restore, API, UI, and regression-test coverage for the new state.

Phase 2 does not include:

- separate per-Agent memory files;
- direct Agent filesystem writes;
- parsing assistant output for magic markers or memory commands;
- background transcript mining;
- semantic clustering performed by another model;
- automatic heartbeat rewrites, inactivity demotion, or time-based deletion;
- automatic changes to User Profile or User Soul;
- model self-reflections or project execution lessons in user memory;
- automatic project/domain-scoped filtering of confirmed Markdown entries.

Project- or domain-specific wording can still be written explicitly in
`user-memory.md`, and a narrower explicit rule wins when an Agent applies two
otherwise conflicting rules. Structured per-project storage and filtering are
deferred until they can be represented without making Markdown parsing the
source of truth.

## Alternatives Considered

### Per-Agent Native Memory

Writing Claude, Codex, Hermes, OpenClaw, and other native memory/config files
would fragment the user's identity, produce different behavior per Agent, and
require tool-specific migrations. Several Agents also have no equivalent
native memory contract. This option is rejected.

### Direct Shell Editing

Giving Agents a path and instructing them to edit Markdown through shell tools
would bypass policy, authentication, size limits, deduplication, concurrency,
and audit metadata. It would also fail for sandboxed Agents. This option is
rejected.

### Assistant-Output Parsing

Scanning normal assistant text for a marker such as `REMEMBER(...)` could
nominally support Pi and OpenClaw, but ordinary or quoted text could trigger a
write, and the host could not distinguish an authorized tool call from model
output. This option is rejected.

### Copying `self-improving-1.2.16` Storage Verbatim

The reference package is useful as a governance model, not as a shared storage
engine. Its correction signals, conservative confirmation, provenance, and
sensitive-data boundaries are retained. Agent-managed files, heartbeat
maintenance, line-count counters, self-reflection promotion, and automatic
30/90-day tier changes are not retained.

### Selected Hybrid

The Rust host remains the sole owner of storage and policy. A private launch
snapshot gives every Agent the same read context. Authenticated MCP tools give
compatible Agents narrowly scoped update operations. The settings UI remains
the universal manual path and the only place that can edit Profile or Soul.

## Canonical Storage

### Active Documents

The three Markdown documents remain the only active memory content injected
into Agent conversations:

- `user-memory.md`: durable facts, explicit preferences, and confirmed reusable
  corrections about the user.
- `user-profile.md`: user-authored identity, background, roles, and stable work
  context.
- `user-soul.md`: user-authored relationship, communication, and behavioral
  values for Agents.

Profile and Soul are advisory user context, not system instructions. System,
developer, project, and current user instructions always take precedence.

Agents may append only to User Memory through a host tool. Profile and Soul
remain read-only to every Agent and manually controlled by the user.

### Candidate State

Unconfirmed learning candidates are stored in a versioned structured state
file owned by `UserMemoryService`:

```text
~/.iyw-claw/.user-memory-learning.json
```

This file is not prompt content and is never parsed as Markdown. It contains
candidate content, lifecycle status, a bounded observation count, timestamps,
and host-derived provenance. It is written atomically under the same process
and filesystem locks used by the canonical documents. The host creates it with
user-only permissions where the operating system supports them and applies the
same symlink and regular-file checks used for the Markdown documents.

Candidate state is kept beside the active documents so desktop memory remains
portable and does not move with an installed application's private database.
The normal UI, rather than direct file editing, is the supported interface for
reviewing this internal state.

The state is bounded as follows:

- 1,000 characters per candidate after normalization;
- 500 candidate records total;
- at most 10 detailed source observations retained per candidate;
- the total observation count may continue increasing after source details are
  capped;
- no automatic deletion or time-based status change.

When the limit is reached, exact duplicate observations may still update an
active existing candidate. A duplicate of a terminal candidate returns that
terminal result without changing it. Creation of a new candidate returns a
bounded error and does not affect the Agent session.

### Policy State

The existing global, per-document, per-Agent, delegation, and Agent-write
policy remains in `app_metadata`. Policy is application configuration, while
the active memory and candidate content live under the canonical memory root.

## Path Resolution

Add `IYW_CLAW_USER_MEMORY_DIR` as the only cross-runtime explicit override for
user-memory content.

### Desktop

Resolution order:

1. Non-empty `IYW_CLAW_USER_MEMORY_DIR`, resolved to an absolute path once at
   startup.
2. The operating-system user home joined with `.iyw-claw`.

Desktop user-memory resolution must not consult `IYW_CLAW_HOME` or
`IYW_CLAW_DATA_DIR`. In particular, `desktop_bootstrap.rs` may set
`IYW_CLAW_HOME=<install-root>/data` for other portable data without moving
memory out of the user's home directory.

If neither an explicit override nor a user home can be resolved, memory
initialization reports an actionable error and disables memory context/tools.
The application and Agent sessions remain usable; memory must not silently
fall back to the current working directory or installation directory.

### Server and Docker

Resolution order:

1. Non-empty `IYW_CLAW_USER_MEMORY_DIR`.
2. Non-empty `IYW_CLAW_HOME` for backwards-compatible deployments.
3. The server's effective persistent data root, normally
   `IYW_CLAW_DATA_DIR` or `/data` in Docker.

The resolved path is absolute and is the single root used by settings, context
injection, Agent updates, backup, and restore.

### Desktop Migration

Migration runs before empty canonical documents are created. It inspects only
known iyw-claw legacy roots derived from startup configuration:

1. an explicitly configured legacy `IYW_CLAW_HOME` captured before desktop
   bootstrap changes it;
2. the former default operating-system home joined with `.iyw-claw`, which is
   relevant when `IYW_CLAW_USER_MEMORY_DIR` now points somewhere else;
3. the installed desktop `<install-root>/data` directory;
4. the effective desktop application data directory.

Absolute duplicate roots and the new canonical root are skipped. The host does
not scan arbitrary disks, other users' home directories, or Agent-native
configuration directories.

For each of the three `user-*.md` files:

- copy only when the canonical file does not exist;
- accept only a regular, non-symlink, valid UTF-8 file within the existing
  64 KiB document limit;
- write the destination atomically under the user-memory lock;
- never overwrite or concatenate an existing canonical file;
- never modify or delete the legacy source;
- if multiple legacy roots contain different content for the same missing
  file, use the first valid root in the order above and expose a warning for
  manual review.

A versioned migration receipt under the canonical root records the considered
sources and a result for each file:

- `copied` and `skipped_existing` are terminal. The host never reimports that
  file automatically, even if the canonical file is later deleted.
- `invalid_source` and `source_missing` are terminal and require manual review.
  A copied file may also list conflicting lower-priority source paths for
  review without changing its terminal `copied` status.
- `source_io_failed` and `destination_io_failed` are retryable on a later
  startup only while the canonical file is still missing.

The receipt is written atomically after each migration pass. This makes
migration idempotent and prevents a user deletion from silently resurrecting
legacy content. Deleting the receipt is an explicit operator action that allows
a fresh migration assessment. A failed file does not prevent other valid files
from being copied, and all failures remain visible in settings and diagnostics.

Candidate state has no legacy equivalent and is not synthesized during
migration.

## Agent Compatibility

All eleven Agents can receive the same read context through the existing
private first-prompt envelope. The table describes each adapter's transport
ceiling when policy, storage, and the companion are healthy; it is not a claim
that every launch has all three capabilities.

| Agent | Read context | Confirmed append | Candidate proposal | Reason |
| --- | --- | --- | --- | --- |
| Claude Code | Yes | Yes | Yes | ACP MCP delivery |
| Codex CLI | Yes | Yes | Yes | ACP MCP delivery |
| OpenCode | Yes | Yes | Yes | ACP MCP delivery |
| Gemini CLI | Yes | Yes | Yes | ACP MCP delivery |
| OpenClaw | Yes | No | No | Rejects non-empty ACP `mcpServers` |
| Cline | Yes | Yes | Yes | ACP MCP delivery |
| Hermes Agent | Yes | Yes | Yes | Built-in companion over ACP MCP |
| CodeBuddy | Yes | Yes | Yes | ACP MCP delivery |
| Kimi Code | Yes | Yes | Yes | Built-in companion over ACP MCP |
| Pi | Yes | No | No | `pi-acp` does not forward wire MCP |
| Grok / 知微 | Yes | Yes | Yes | Built-in companion over ACP MCP |

OpenClaw and Pi remain intentionally read-only until their adapters provide a
safe, verifiable tool transport. Their sessions must not be given update
instructions for tools they cannot call. Users can edit `user-memory.md` or
review/add content in settings regardless of which Agent produced the useful
information.

If a normally writable Agent launches without a ready companion, both tool
capabilities become unavailable while readable context remains independent.
This is a runtime capability failure, not a policy change, and it does not
clear the user's configured intent.

### Effective Capability Vector

The backend exposes three independent results, each with `available`, a stable
reason code, and optional degraded reasons:

- `readContext`: policy/origin allows injection and at least the enabled,
  readable documents can be snapshotted. One unreadable document produces a
  degraded result and is omitted without hiding other readable documents.
- `confirmedAppend`: Agent updates are enabled, the User Memory document is
  enabled and writable, the adapter delivers MCP, and the ready companion
  advertises `append_user_memory`.
- `candidateProposal`: Agent updates are enabled, candidate state is valid and
  writable, the adapter delivers MCP, and the ready companion advertises
  `propose_user_memory`.

Global, per-Agent, delegation-origin, and relevant document policy gates are
applied before resource and transport checks. A read-only User Memory document
can disable confirmed append while candidate proposal remains available. An
invalid candidate store disables proposal while confirmed append remains
available. Companion failure disables both tools but does not by itself disable
read context.

Settings exposes a projected vector for a new session using current health.
`LiveSessionSnapshot` exposes the actual immutable vector captured by each
launched session. Policy intent and runtime capability are never represented by
the same boolean.

## Read Context

Phase 1 injection behavior remains authoritative:

- Build a bounded snapshot at real connection launch.
- Inject it once before the first accepted prompt.
- Never inject it into probe connections.
- Keep the original prompt unchanged in events, titles, ledgers, and UI.
- Strip the private sentinel from parsed conversation content.
- Do not inject a second envelope into resumed or already-running sessions.
- Apply settings and document changes fully on a new conversation.

Candidates, migration receipts, source metadata, and companion diagnostics are
never included in the private context.

The maintenance section is rendered from the effective launch vector, not only
from policy. It names only tools that were successfully injected. Read context
remains available when either or both update tools are unavailable.

Launch sequencing is therefore explicit:

1. Load one immutable policy/document snapshot without rendering maintenance
   guidance.
2. Initialize the Agent and attempt companion discovery, validation, and ACP
   injection.
3. Compose the three capability results, expose only the available memory tool
   features, finalize context guidance, then compute the effective fingerprint
   and store the vector and context in `SessionState`.
4. Mark the connection ready and accept the first user prompt only after that
   final snapshot exists.

A reused or resumed connection keeps its already-finalized launch snapshot.
Companion health changes do not mutate a live session's prompt history.

The settings page must state that enabled context is sent to the selected model
provider and may remain in the Agent's native session history. Hiding the
private envelope in iyw-claw's UI does not mean it was never transmitted.

## Update Signals

### Direct Confirmed Memory

The existing `append_user_memory` tool remains backwards compatible and keeps
the `{ content }` schema. It is appropriate only when the current user has
clearly supplied a durable, cross-task fact or preference, including:

- an explicit request to remember something;
- an explicit stable fact about the user, such as a preferred name;
- an explicit `always`, `never`, or equivalent lasting preference;
- an explicit confirmation of a previously discussed candidate.

The Agent should call the tool during the same turn once the durable signal is
clear. It should not claim persistence when the call is unavailable or fails.
The host continues to normalize, bound, authenticate, deduplicate, and append a
single provenance-bearing entry to `user-memory.md`. In the same locked
transaction, it marks any active candidate with the same normalized content as
`confirmed` and records the resulting entry identifier. The implementation
uses a lock-aware inner append primitive rather than recursively acquiring the
user-memory lock. If candidate state is unreadable, the backwards-compatible
Markdown append still proceeds after its normal checks, reconciliation is
skipped, and settings records a repairable candidate-state diagnostic.

### Conservative Candidate

Add `propose_user_memory` with this Agent-visible input:

```json
{
  "content": "concise reusable statement",
  "signal": "correction"
}
```

`signal` is one of `correction`, `preference`, or `fact`. The Agent cannot set
the destination file, path, Agent identity, lifecycle status, observation
count, timestamps, or source identifiers.

The proposal tool is appropriate when the user gives an explicit correction
or preference that appears reusable but has not clearly made it permanent. A
single task result, silence, an Agent's own reflection, or an inferred trait is
not a candidate signal.

Submitting a candidate does not change prompt context. The tool result reports
whether the observation was new, its current status, and whether user
confirmation is recommended. The Agent may ask a short natural-language
confirmation question when it fits the conversation, but it must not interrupt
the active task or state that the candidate is already remembered.

### Manual Fallback

The existing settings editor remains available for all Agents and runtimes.
It is the fallback for Pi, OpenClaw, missing companions, rejected tool calls,
and users who disable Agent updates. No host process watches or parses normal
Agent output to emulate a tool call.

## Candidate Lifecycle

Candidate content is normalized by trimming, collapsing whitespace, rejecting
control characters, and using a case-insensitive digest. The deduplication key
contains the normalized content and signal type.

For each accepted user prompt, the host increments a per-connection monotonic
memory-turn nonce before forwarding the prompt. The connection token exposes a
read-only handle to a tracker containing that nonce and an active flag. The
flag is cleared on turn completion, cancellation, or terminal error; proposal
calls without an active accepted turn are rejected. A candidate observation is
idempotent for `(candidate digest, launch token, turn nonce)`. The raw token is
never persisted; structured provenance stores a hash-derived opaque source
identifier. Repeated calls in one turn therefore count once, while later
prompts on the same connection can provide distinct observations.
Client-supplied message IDs are not used as the authority.

States are:

1. `tentative`: one distinct observation.
2. `emerging`: two distinct observations of the same normalized candidate.
3. `pending_confirmation`: three or more distinct observations.
4. `confirmed`: the user explicitly confirms it in conversation through the
   direct append flow, or confirms it in settings.
5. `rejected`: the user rejects the candidate; it is never injected.
6. `superseded`: the user resolves it in favor of another candidate or an
   existing confirmed rule.

Allowed transitions are explicit:

| Current state | Automatic distinct observation | User resolution |
| --- | --- | --- |
| New | `tentative` | Not applicable |
| `tentative` | `emerging` | Confirm, reject, or supersede |
| `emerging` | `pending_confirmation` | Confirm, reject, or supersede |
| `pending_confirmation` | Stay pending; count +1 | Confirm/reject/supersede |
| `confirmed` | No mutation | Delete terminal record only |
| `rejected` | No mutation | Delete terminal record only |
| `superseded` | No mutation | Delete terminal record only |

Terminal duplicates return the existing identifier/status without adding an
observation. A superseded record stores either `supersededByCandidateId` or
`supersededByMemoryEntryId`; exactly one is required. Deleting a terminal record
allows a later matching proposal to begin a new lifecycle, which is an explicit
user choice.

Observation counts never cause automatic confirmation. Similar but
non-identical phrases are not merged automatically. The UI may let the user
edit and merge wording before confirmation, but the host does not use an LLM
to infer semantic equivalence.

Confirming in settings performs one locked transaction:

1. validate the candidate revision and optional edited text;
2. append through the same `UserMemoryService` path used by
   `append_user_memory`;
3. record the resulting deterministic memory entry identifier;
4. change the candidate status to `confirmed`;
5. atomically persist the Markdown and structured state, or roll both back.

Rejecting or superseding a candidate updates only structured state. A terminal
record remains visible until the user explicitly removes it. There is no
automatic 30-day demotion, 90-day archive, or heartbeat cleanup.

## Transaction and Crash Recovery

The durable journal is extended to cover policy, affected Markdown documents,
and candidate state. Every multi-resource update stores previous and next
generations plus a transaction identifier and phase.

The write sequence under the process and filesystem locks is:

1. Recover any existing journal before reading current state.
2. Write and fsync a `prepared` journal containing validated previous and next
   generations.
3. Atomically replace all next-generation files in a deterministic order and
   persist next policy when the operation includes policy.
4. Atomically replace and fsync the journal with phase `committed`.
5. Remove the journal and fsync the root directory.

Recovery follows this complete truth table:

| Journal state | Recovery action |
| --- | --- |
| Absent | Validate and use the current files/policy |
| Valid `prepared` | Restore every previous generation, then remove journal |
| Valid `committed` | Reapply every next generation, then remove journal |
| Invalid or mismatched | Fail closed and report a repairable error |

A crash after next files are written but before the committed marker therefore
rolls back; a crash after the committed marker rolls forward. Recovery is
idempotent. Single-file candidate observations may use one atomic replacement
without a journal. Candidate confirmation, a direct append that reconciles a
candidate, document-plus-policy edits, backup snapshots, and restore all use
the full transaction/lock contract.

## Provenance and Auditability

Tool authentication binds every operation to the launch token, Agent type,
connection, and launch-time policy. The model cannot provide or override those
fields.

Candidate provenance stores only what is needed for review:

- Agent type;
- opaque conversation/connection reference;
- observation timestamp;
- signal type;
- bounded candidate text.

It does not store raw prompts, full transcripts, credentials, companion tokens,
or arbitrary filesystem paths. Confirmed Markdown entries retain the existing
UTC timestamp, Agent display name, and deterministic entry marker.

Manual document edits remain authoritative even when they remove or change an
Agent-created entry. The host does not reconstruct deleted Markdown from audit
state.

## Conflict Handling

Instruction precedence is always:

1. system, developer, and project instructions;
2. the current explicit user request;
3. the enabled private user context.

Within user context, an explicit narrow rule applies over a general rule. A
new current-user correction applies for the current turn even before it is
confirmed as memory.

The backend does not attempt semantic conflict detection. When an Agent notices
that a new statement conflicts with injected memory, it must follow the current
statement and submit a candidate rather than silently adding a second confirmed
rule. The settings review flow lets the user edit the canonical Markdown and
mark obsolete candidates as superseded.

Exact normalized duplicate appends remain idempotent. Optimistic revisions and
document etags protect UI saves and candidate resolution from concurrent
changes.

## Security and Privacy

Neither direct memory nor candidates may contain:

- passwords, API keys, tokens, private keys, recovery codes, or credentials;
- raw authentication/configuration payloads;
- inferred health, race, ethnicity, religion, sexuality, political belief, or
  other sensitive traits;
- third-party private facts presented as the user's memory;
- repository facts, source code, command output, or temporary work progress;
- one-off task details, transient errors, or Agent self-evaluation.

Explicit non-secret personal facts supplied by the user, such as a preferred
name or form of address, are allowed. The host retains deterministic checks for
common credential formats and rejects invalid length/control characters, while
the tool contract and user review provide the semantic boundary.

The service rejects symlinked canonical documents and structured state,
read-only update targets, path traversal, and writes outside the resolved root.
Agents never receive the root path as a writable instruction.

## Companion Health and Graceful Degradation

Expose a shared companion health state:

```text
ready | missing | incompatible | probe_failed | timeout
```

Health includes a stable reason code, expected/detected version where known,
selected executable path, and the advertised tool manifest. The capability
probe runs outside the async executor's blocking path and has a short bounded
timeout. A hung or malformed binary cannot leave a conversation indefinitely
in `Connecting`.

Companion health is one input to each tool capability, not a combined
read/write state. The launch feature `memory` exposes confirmed append only;
the separate launch feature `memory-proposal` exposes candidate proposal only.
The companion filters its tool list accordingly. A ready companion can
therefore expose one tool without the other when document or candidate-store
health differs. Non-ready states emit a diagnostic and allow the Agent
connection to continue without companion-backed memory tools.

Release packaging and development sidecar preparation validate the complete
capability manifest, including `append_user_memory` and
`propose_user_memory`, rather than checking only that the executable starts.
Native-target jobs execute the packaged binary's `--capabilities` command.
Cross-target jobs that cannot execute a foreign binary validate target naming,
binary inclusion, and a source-derived capability manifest; they must not
report that as a runtime probe. The first launch on the target remains the
authoritative executable check and degrades safely on mismatch.

## Policy and API

The existing `enabled`, `agentWriteEnabled`, `inheritToSubagents`, per-Agent,
and per-document controls remain. `agentWriteEnabled` gates both confirmed
append and candidate proposal. Disabling it does not delete existing candidates
or confirmed content.

Extend the settings snapshot with:

- resolved canonical root and its source (`override`, `desktop_home`,
  `server_home`, or `server_data`);
- migration results and warnings;
- per-Agent `readContext`, `confirmedAppend`, and `candidateProposal` results,
  each with stable reason/degraded codes;
- companion health;
- candidate counts by lifecycle status.

The live-session snapshot carries the same three fields using the actual
launch result. Projected settings capability is labeled as applying to a new
conversation.

Add shared Tauri/core/HTTP operations:

- `list_user_memory_candidates`: paginated, filterable candidate summaries;
- `resolve_user_memory_candidate`: confirm, reject, supersede, or edit-and-
  confirm using an expected state revision;
- `delete_user_memory_candidate`: explicitly remove a terminal candidate using
  an expected state revision.

All Tauri and HTTP handlers delegate to the same core service methods. Static
export constraints remain unchanged; no dynamic Next.js route is introduced.

Existing clients that only understand the Phase 1 settings fields continue to
work. The `append_user_memory` MCP schema is unchanged. The new proposal tool
and API fields are additive.

## Settings Experience

The existing User Memory settings page remains the single management surface.
It adds:

- the backend-resolved canonical root and storage source;
- separate read-context, confirmed-append, and candidate-proposal capability
  for each Agent;
- a precise unavailable/degraded reason for Pi, OpenClaw, unhealthy companions,
  read-only documents, and invalid candidate state;
- migration warnings with source and per-file outcome;
- candidate filters for tentative, emerging, pending, and terminal states;
- confirm, edit-and-confirm, reject, supersede, and terminal-delete actions;
- source Agent, observation count, and first/last observation time.

The page does not imply that the global write policy guarantees either runtime
tool. Policy describes user intent; projected capability describes what a new
session can currently do, while a live-session view reports its launch
snapshot.

Profile and Soul remain ordinary document editors with explicit save. Agent
candidate controls appear only for User Memory.

## Backup and Restore

Backup continues to store canonical memory under the stable `user-memory/`
archive prefix. Phase 2 adds `.user-memory-learning.json`. The migration receipt
and live lock/journal files are excluded.

Restore accepts older archives containing only the three Markdown documents.
Missing candidate state means an empty candidate list. Candidate state is
validated and restored under the same memory lock. Restore never trusts an
archive path or environment value to choose a destination; it uses the current
runtime's resolved canonical root.

## Error Handling

- An unavailable desktop home disables memory and exposes a configuration
  error without preventing normal Agent sessions.
- An unreadable canonical document disables only the affected memory snapshot
  and surfaces the path/error; it is never replaced silently.
- Invalid candidate JSON is preserved unchanged, candidate writes are disabled,
  and settings reports a repairable error. The host does not reset it to empty.
  Active Markdown reads and confirmed appends continue; appends skip candidate
  reconciliation until the structured state is repaired.
- A stale settings, document, or candidate revision returns `conflict` and
  requires reload.
- A candidate limit or validation failure is returned to the calling tool but
  does not fail the Agent turn.
- A companion failure makes both tool capabilities unavailable and never
  disables context that was otherwise readable.
- A partial confirm/update is recovered or rolled back using the extended
  durable transaction journal.

## Verification Strategy

Permanent regression tests are part of the implementation and must remain in
the repository. Temporary smoke scaffolding, ad-hoc test binaries, temporary
fixtures outside the normal test harness, and generated verification files are
deleted after their evidence is captured.

Required coverage:

1. Desktop path tests prove `IYW_CLAW_HOME` cannot move memory and
   `IYW_CLAW_USER_MEMORY_DIR` can.
2. Server path tests cover override, backwards-compatible home, and data-root
   fallback.
3. Migration tests cover missing-only copy, precedence, symlinks, oversize and
   invalid UTF-8 files, conflicts, partial failure, and idempotency.
4. A table-driven eleven-Agent test pins read, confirmed append, and proposal
   capabilities, including Pi and OpenClaw read-only behavior.
5. Companion tests cover complete manifests, missing/stale binaries, malformed
   output, and probe timeout.
6. Candidate tests cover normalization, same-turn idempotency, distinct-source
   counts, every lifecycle transition, limits, and no automatic confirmation.
7. Transaction tests prove candidate confirmation cannot leave Markdown and
   structured state in different generations.
8. Context tests prove candidates and diagnostics are never injected and
   maintenance guidance matches actual launch capability.
9. Capability-composition tests cover companion failure, read-only Memory,
   partial document reads, invalid/read-only candidate state, disabled policy,
   and every one-tool-only combination.
10. Security tests cover credential patterns, control characters, forged Agent
   identity/status/path fields, symlinks, and authorization policy.
11. Backup/restore tests cover new and Phase 1 archive shapes.
12. Frontend tests cover capability badges, migration warnings, candidate
    actions, conflicts, loading/error states, and all translated labels.
13. Focused Rust, frontend, lint, build, desktop, server, and MCP checks run in
    proportion to the touched modules before completion.

## Acceptance Criteria

1. A default Windows desktop resolves active memory to
   `%USERPROFILE%\.iyw-claw`, including an installed portable build.
2. `IYW_CLAW_HOME=<install-root>/data` does not change the desktop memory root.
3. `IYW_CLAW_USER_MEMORY_DIR` is honored consistently by settings, context,
   tools, backup, and restore.
4. Legacy desktop documents are copied only when missing, never overwritten or
   deleted, and migration outcomes are visible.
5. All eleven Agents receive the same enabled first-prompt context; no probe or
   visible transcript contains the private envelope.
6. The nine MCP-capable Agents expose confirmed append when the Memory document
   is writable, and candidate proposal when candidate state is healthy, if the
   corresponding policy and ready companion manifest allow each tool.
7. Pi and OpenClaw stay functional and read-only without receiving false tool
   guidance.
8. A missing, stale, malformed, or hung companion degrades safely and reports a
   stable reason.
9. An explicit durable user fact or preference can be appended during the same
   turn through the authenticated existing tool.
10. An uncertain reusable correction can be proposed without becoming active
    memory.
11. Repetition can move a candidate to pending confirmation but can never
    confirm it automatically.
12. Candidate observation counts never write `user-memory.md`; promotion
    requires either an explicit settings resolution or the separately
    authenticated confirmed-append operation, whose tool contract requires an
    explicit user signal.
13. Profile and Soul cannot be changed by any Agent tool.
14. Deterministic secret/control-character filters reject their documented
    patterns, and every Agent-facing update contract explicitly forbids
    inferred sensitive traits, project state, and Agent reflections.
15. Concurrent settings edits, Agent updates, candidate proposals, candidate
    confirmation, backup, and restore cannot silently lose data.
16. Active memory and candidate content remain under the resolved canonical
    memory root on desktop.
17. No automatic heartbeat, output parsing, direct Agent file edit, inactivity
    demotion, or background transcript learning is introduced.
18. All required permanent regression tests pass, and temporary test artifacts
    created solely for verification are removed before delivery.
