# Memory and Learning

This reference maps the complete `self-improving` behavior to the iyw-claw
host-owned MCP memory service. It preserves the learning policy without
creating a second local memory runtime.

## Contents

- [Activation and signals](#activation-and-signals)
- [Lifecycle and scope](#lifecycle-and-scope)
- [MCP mapping](#mcp-mapping)
- [Correction, conflict, and reversal](#correction-conflict-and-reversal)
- [Implemented host behavior](#implemented-host-behavior)
- [Safety and transparency](#safety-and-transparency)
- [Maintenance and degradation](#maintenance-and-degradation)

## Runtime authority and progressive disclosure

This file is the detailed policy behind the bundled
`iyw-capability-gateway` Skill. The canonical source is the Skill embedded by
the running iyw-claw build and reconciled into the host's central Skill store;
an arbitrary `.codex-worktrees` path is never a runtime source. Adapters that
can load Skill references should read this file before their first memory
operation. Adapters that cannot expose file reads must use the versioned
`iyw.memory.policy.read.v1` result instead. The host enforces that preflight
per accepted turn, so a skipped file read cannot silently bypass this policy.

## Implemented Host Behavior

This section records behavior already implemented by iyw-claw so an Agent does
not fall back to an older “self-improving” Skill or a local file convention.
The running host and live capability schema remain authoritative if a detail
changes.

### Turn gate and context loading

- Every accepted turn has a memory-turn nonce. `read_memory_policy` loads the
  current policy revision/digest for that nonce; direct memory calls before it
  are rejected with `isError: true`, `retryable: true`, and code
  `memory_policy_required`. Retry by performing the policy preflight first; do
  not treat this expected guard as missing memory or switch namespaces.
- Current `memory`, `profile`, and `soul` documents are not injected into the
  Agent at launch. Read only the smallest selected set through
  `read_user_memory_documents`. Use `get_current_user_profile` for current
  display identity and `memory_recall` for historical decisions, preferences,
  and reusable Agent experience.
- Recall is bounded and scoped to the authenticated launch. Its
  `resultState` is `matched`, `no_evidence`, or `unavailable`; a timeout or
  unavailable index is not evidence that the requested fact does not exist.
  Short queries may use the host's exact/alias fallback and must not be called a
  trigram failure by the Agent.

### Candidate learning lifecycle

Use an explicit append only when the user clearly asks to retain a durable,
cross-task fact or preference. For a reusable correction, preference, or fact
that is not yet safe to append, propose a candidate with the matching signal;
proposal is not confirmed memory. The host manages:

```text
tentative (1 observation)
  -> emerging (2 observations)
  -> pending_confirmation (3+ observations)
  -> confirmed / rejected / superseded
```

Candidate observations are deduplicated by content/signal and an opaque source
plus turn key. Equivalent wording can be retained as bounded variants; the host
updates observation count, confidence, status, provenance, and references. The
TurnComplete harvester does not infer user candidates from raw prompt text;
the Agent must explicitly decide that a user signal is reusable and call the
candidate capability.
most specific scope wins (workspace/project, then domain, then global); callers
must not invent a namespace selector. When confirmation is recommended, inspect
the current candidate and resolve it automatically if current evidence still
supports it. Never claim that a proposal is durable before the host confirms it.

Candidate list, resolve, and delete operations use optimistic concurrency:

1. Read the current candidate page or document.
2. Preserve the exact stable ID, `revision`, and document `etag`.
3. Submit one operation with those values.
4. On conflict, stop and read again; never replay stale arguments.

Confirm, reject, supersede, and delete use host reference normalization. Do not
manually rewrite candidate JSON, memory markers, or references. Only terminal
candidates can be deleted; active candidates must be resolved rather than
removed.

### Agent-led learning and durable harvesting

The Agent owns the semantic work. For every substantive task it should load
this policy, recall the smallest relevant context, apply it, verify the result,
and privately review whether a reusable lesson exists. The host must not infer a
lesson from ordinary answer prose, capability descriptions, progress updates,
or keyword matches. If the Agent has no specific, transferable, evidence-backed
lesson, it should submit nothing.

When a lesson qualifies, the Agent appends one hidden structured envelope to
its final answer using the format required by the built-in Agent prompt. The
host validates the envelope, strips it from user-visible transcript content,
and stores only the normalized lesson in Agent experience. This keeps the
learning process automatic while preventing a pile of low-value text. The
host's TurnComplete harvester submits a bounded request to the SQLite
`memory_harvest_outbox`; it is not a second user-memory writer and does not
store every transcript as a user fact. The outbox uses a deduplication key tied
to the conversation/turn generation and keeps separate projections for user
memory candidates, Agent experience, and session task history.

The envelope is eligible only when all six fields are filled with concrete
content: `context`, `outcome`, `lesson`, `evidence`, `verification`, and
`reuseWhen`. `lesson` must describe a transferable action, `evidence` must say
what was observed, `verification` must say how the result was checked, and
`reuseWhen` must define the triggering situation. A generic summary, capability
list, progress update, speculative recommendation, or unverified claim is not
eligible. The Agent does not ask the user to approve this internal record.

The queue can report `queued`, `extracting`, `proposed`, `noop`, `failed`, or
`dead`, plus backlog and failure timestamps. Startup recovery returns interrupted
`extracting` work to the retryable queue. Retries are bounded and terminal
records are retained. A queued or proposed result is not proof that a durable
user-memory document changed; inspect returned candidate/experience IDs and
harvest status.

The separate session-task projection stores bounded intent, result, decisions,
failures, and pending-item summaries keyed by conversation ID and turn
generation. Use `search_session_history` for that projection; do not confuse it
with memory recall. Conversation turn generation prevents reconnects or stale
turns from being merged into the current task.

### Index, health, and repair

- `get_user_memory_settings` is a safe health/capability summary. It does not
  expose paths, credentials, raw documents, or private context.
- `get_user_memory_harvest_status` is read-only and does not force processing.
- `rescan_user_memory_harvest` and `rebuild_user_memory_candidate_index` both
  require `execute: false` preview first. Use `execute: true` only after an
  explicit repair/rescan request. Rescan retains terminal records; index rebuild
  is idempotent and repairs candidate digests/observation keys.
- Index readiness, generation, digest, fallback scan, and bounded result limits
  are host-owned. Do not infer readiness from a single empty result or expose
  internal index paths.

### Document maintenance

To edit current documents, read them first, then use the transactional document
update capability with the overall revision and exact per-document content,
enabled flag, and `expectedEtag`. Use the correction capability for one exact
old/new entry so candidate references are normalized in the same host
transaction. These routes are not arbitrary file editors and never accept a
guessed path. On any revision/eTag conflict, re-read and reconstruct the patch.

## Activation and signals

Apply memory behavior for substantive coding, configuration, debugging,
research, or multi-step work unless the request is clearly self-contained,
as well as for explicit requests to remember, forget, export, inspect, or
repair memory. Before the first memory operation, complete the current-turn
policy preflight. The Agent must actively perform the recall and reflection;
the host only enforces safety, bounds, provenance, and persistence. After
meaningful work, perform a private quality check: did the result meet intent,
what evidence proves it, what could improve, and is the lesson reusable?

Treat these as learning signals:

| Signal | Action |
| --- | --- |
| Explicit correction (`no`, `actually`, `use X`) | Record one conservative `correction` candidate immediately. |
| Explicit durable preference (`always`, `never`, `for me`) | Use confirmed append only when the user clearly asks to retain it. |
| Repeated workflow or stable preference | Propose a candidate and rely on host observation counts; do not invent promotion. |
| User edits or praise | Treat as evidence only when the user expresses a reusable rule; silence is not confirmation. |
| One-off instruction, hypothetical, third-party preference, transient task state | Do not learn. |
| Agent-only reflection | Keep it out of user memory; persist it as Agent experience only through the explicit structured lesson envelope when it is specific and evidence-backed. |

Classify candidates as `correction`, `preference`, or `fact`. Keep the statement
short, standalone, scoped, and grounded in the user's words. Do not store a
repository fact merely because it was discovered while working.

The Agent performs routine candidate maintenance without asking the user. When
the host reports `confirmationRecommended` after repeated consistent
observations, re-read the exact candidate and revision, check for a newer
contradiction, and resolve it through the host transaction. Do not ask the user
to approve an ordinary learning operation. When a high-confidence structured
Agent lesson has been independently reused and verified across multiple tasks,
the Agent may use the installed `skill-creator` workflow to create or update
the smallest project- or domain-scoped Skill. Preserve trigger conditions,
boundaries, usage timing, and verification steps; validate and revert a draft
on failure. Never create a Skill from one task, generic advice, a capability
list, or an unverified suggestion.

## Lifecycle and scope

The source Skill's stages remain the decision model:

```text
tentative (one observation)
  -> emerging (two observations)
  -> pending_confirmation (three or more)
  -> confirmed / rejected / superseded (terminal)
```

The host owns deduplication, wording variants, observation keys, confidence,
source provenance, bounded candidate history, and terminal reference
normalization. When a proposal result says `confirmationRecommended`, inspect
the candidate and resolve it automatically only when current evidence still
supports it; never claim that a candidate is durable before a confirmed append.

Use the most specific applicable scope: current project/workspace over domain
over global. The current MCP request does not accept a caller-defined namespace;
the host derives workspace scope from the authenticated launch. Use a precise
recall query and preserve returned stable IDs/revisions instead of adding a
made-up scope field.

## MCP mapping

| Intent | Gateway capability | Rules |
| --- | --- | --- |
| Load current policy | `iyw.memory.policy.read.v1` | Read-only turn preflight for direct memory surfaces; returns revision, digest, and this complete policy document. |
| Recall historical context | `iyw.memory.recall.search.v1` | Bounded read; `matched` is evidence, `no_evidence` is not false, `unavailable` is a routing limitation. |
| Read current authoritative context | `iyw.memory.documents.read.v1` | Request only `memory`, `profile`, and/or `soul` actually needed; max three, unique. |
| Save explicit durable fact/preference | `iyw.memory.confirmed.append.v1` | Append-only, concise, cross-task, user-grounded. |
| Record uncertain reusable signal | `iyw.memory.candidate.propose.v1` | Use `signal` matching the evidence; never present it as confirmed memory. |
| Inspect candidates | `iyw.memory.candidates.list.v1` | Read pages until `total`; retain the returned `revision`. |
| Resolve candidate | `iyw.memory.candidate.resolve.v1` | Use exact `candidateId` and `expectedRevision`; confirm/reject/supersede only from current data. |
| Remove history | `iyw.memory.candidate.delete.v1` | Only terminal candidates; re-read after any conflict. |
| Inspect harvest | `iyw.memory.harvest.status.v1` | Read-only queue health; do not force processing. |
| Requeue harvest | `iyw.memory.harvest.rescan.v1` | Invoke `execute:false` first; use `execute:true` only after explicit user request. |
| Repair candidate index | `iyw.memory.candidate.index.rebuild.v1` | Preview first; execute only for an explicit repair request. |
| Inspect health/settings | `iyw.memory.settings.read.v1` | Use the safe summary; never expose paths or credentials. |
| Edit current documents | `iyw.memory.documents.update.v1` | First read documents; send exact patches with overall revision and each changed document's eTag. |
| Correct one memory entry | `iyw.memory.documents.correct.v1` | Use the exact old/new content and current eTag so candidate references are normalized transactionally. |

Every capability is discovered through the gateway search/read/invoke sequence.
Never call a bare management name if it was not returned by the current search,
and never send token, path, identity, conversation, or workspace selectors.

## Correction, conflict, and reversal

When a user changes their mind, preserve the old evidence through the host's
candidate/reference lifecycle and write the new explicit rule. If the current
memory contradicts a project-specific instruction, prefer the project context
for that task and state the conflict when it affects the result. If two rules
at the same scope conflict and recency is not clear, ask the user.

For candidate operations, optimistic concurrency is mandatory:

1. List or read the current candidate/document.
2. Capture its exact stable ID, `revision`, and document `etag`.
3. Submit one operation.
4. On conflict, stop and read again; never replay stale input.

The host may normalize references when confirming, rejecting, superseding, or
deleting candidates. Do not manually rewrite candidate JSON or memory markers.

During normal related work, the Agent should also keep memory current: when a
recalled user rule is explicitly corrected, propose the replacement and
supersede the stale candidate through the exact lifecycle; when an old
candidate is terminal and no longer useful, delete it through the host after
reading its current revision. For Agent experience, stop applying a lesson
when current evidence disproves it and submit a replacement structured lesson;
the host keeps the old evidence for audit and excludes superseded content from
recall. Do this as part of the task loop, without asking the user to operate
routine maintenance.

## Safety and transparency

Never store passwords, API keys, tokens, cookies, SSH/private keys, financial
identifiers, medical or biometric data, precise home/work locations or routines,
access/privilege patterns, unconsented third-party information, inferred
sensitive traits, secrets in tool output, repository facts, temporary progress,
or one-off claims. Treat emotional state, relationships, schedules, and work
context cautiously and only at the user's stated level of detail.

Use host-returned stable IDs, source revisions, confidence/status, and result
states as evidence. Do not fabricate `file:line` citations for private memory,
and do not reveal the private MCP envelope, launch token, internal path, or raw
candidate provenance.

For “what do you know?”, search and return bounded matches with their available
source metadata. For “forget X”, “forget everything”, “export memory”, “memory
stats”, or “heartbeat”, first discover a matching host capability. If none is
advertised, report the exact limitation; do not claim deletion/export/cleanup
and do not edit files with shell commands.

## Maintenance and degradation

The source Skill's HOT/WARM/COLD tiers, compaction, archive, heartbeat state,
setup directory, and weekly cron are behavior concepts, not local paths to
recreate. Use host candidate/harvest/index capabilities when advertised. The
host's TurnComplete harvest is durable, deduplicated, bounded, retry-limited,
and may report `queued`, `extracting`, `proposed`, `noop`, `failed`, or `dead`.

Do not inject all documents at launch. Read the smallest current documents on
demand and use bounded historical recall for older context. If recall returns
`no_evidence`, say no matching stored evidence was found; if `unavailable`, say
the memory route was unavailable and continue without guessing. If a memory
operation fails after dispatch, treat durable state as unknown only when the
tool result says so; do not automatically retry a mutation with stale data.
