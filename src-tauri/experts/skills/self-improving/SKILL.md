---
name: self-improving
description: Use when the user explicitly corrects an Agent, states a durable preference or fact, asks the Agent to remember something, or when the Agent needs to record a non-user operational reflection after significant work.
---

# Self Improving

Improve conservatively through iyw-claw's existing user-memory system. Never
create a second durable user-memory database.

## Memory Contract

iyw-claw owns these durable documents under `~/.iyw-claw/`:

- `user-memory.md`
- `user-profile.md`
- `user-soul.md`

Do not edit these files with shell commands or file-edit tools. Use the host
memory tools so policy, review, deduplication, backup, and user controls remain
authoritative:

- Call `append_user_memory` only when the user clearly asks to remember a
  durable cross-task fact or preference.
- Call `propose_user_memory` for an explicit correction, preference, or fact
  that appears reusable but still needs user review.
- If the relevant tool is unavailable, do not write the memory files directly.
  Continue without persistence and state that no durable memory was changed.

Never store secrets, credentials, inferred sensitive traits, repository facts,
temporary progress, one-off instructions, third-party personal information, or
the Agent's private reasoning.

## Operational State

Agent-authored reflection state is separate from user memory and may live only
under:

```text
~/.iyw-claw/self-improving/
|- heartbeat-state.md
`- reflections.md
```

This directory may record non-sensitive process lessons and maintenance times.
It must not contain user facts or preferences and is never injected as user
memory. Read [setup.md](setup.md) only when the state directory is missing.

## Signal Routing

| Signal | Action |
|---|---|
| "Remember that I prefer..." | `append_user_memory` |
| Explicit correction without a remember request | `propose_user_memory` with `correction` |
| Explicit reusable preference | `propose_user_memory` with `preference` |
| Explicit reusable fact | `propose_user_memory` with `fact` |
| One-time task instruction | Follow it now; do not persist |
| Agent notices a better execution method | Append a concise entry to operational `reflections.md` |
| Silence or inferred behavior | Do not learn from it |

## Reflection Workflow

After significant work, a failure, or a user correction:

1. Compare the result with the user's explicit request.
2. Identify one concrete improvement, if any.
3. Route user-provided durable information through the host memory tools.
4. Route Agent-authored process improvement only to operational
   `~/.iyw-claw/self-improving/reflections.md`.
5. Do not create churn when there is no reusable lesson.

Use this operational format:

```text
Date: 2026-07-22
Context: settings migration
Observation: hidden controls remained callable through the backend
Next time: enforce hidden policy at the API boundary
```

Read [operations.md](operations.md) for query/forget behavior and
[boundaries.md](boundaries.md) for privacy constraints.

## Hard Boundaries

- Never install companion skills or modify workspace instruction files as part
  of setup.
- Never promote an Agent reflection into user memory.
- Never claim a proposal is already remembered.
- Never delete or rewrite user memory in response to a general cleanup task.
- For forget/export requests, use iyw-claw's user-memory UI or supported host
  operations; do not manipulate storage files directly.
