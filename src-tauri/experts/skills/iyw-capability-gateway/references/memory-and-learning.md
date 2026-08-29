# Memory and Learning

This reference maps the complete `self-improving` behavior to the iyw-claw
host-owned MCP memory service. It preserves the learning policy without
creating a second local memory runtime.

## Contents

- [Activation and signals](#activation-and-signals)
- [Lifecycle and scope](#lifecycle-and-scope)
- [MCP mapping](#mcp-mapping)
- [Correction, conflict, and reversal](#correction-conflict-and-reversal)
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

## Activation and signals

Apply memory behavior for substantive coding, configuration, debugging,
research, or multi-step work unless the request is clearly self-contained,
as well as for explicit requests to remember, forget, export, inspect, or
repair memory. Before the first memory operation, complete the current-turn
policy preflight. After meaningful work, perform a private quality check: did
the result meet intent, what could improve, and is the lesson reusable?

Treat these as learning signals:

| Signal | Action |
| --- | --- |
| Explicit correction (`no`, `actually`, `use X`) | Record one conservative `correction` candidate immediately. |
| Explicit durable preference (`always`, `never`, `for me`) | Use confirmed append only when the user clearly asks to retain it. |
| Repeated workflow or stable preference | Propose a candidate and rely on host observation counts; do not invent promotion. |
| User edits or praise | Treat as evidence only when the user expresses a reusable rule; silence is not confirmation. |
| One-off instruction, hypothetical, third-party preference, transient task state | Do not learn. |
| Agent-only reflection | Keep it internal unless the user explicitly turns it into a reusable preference/fact. |

Classify candidates as `correction`, `preference`, or `fact`. Keep the statement
short, standalone, scoped, and grounded in the user's words. Do not store a
repository fact merely because it was discovered while working.

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
the candidate and ask for confirmation only if a user decision is actually
needed; never claim that a candidate is durable before a confirmed append.

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
