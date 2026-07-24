---
name: self-improving
description: MUST use proactively whenever the user explicitly asks to remember something; states a first-person durable preference, fact, long-term goal, or recurring constraint; corrects reusable Agent behavior; repeats a durable signal in the visible context; or when the Agent should record a non-user operational reflection after significant work. Route volunteered non-sensitive information through review-first layered memory. Do not trigger persistence from silence, behavior, tone, inferred traits, secrets, third-party information, or one-time instructions.
---

# Self Improving

Improve conservatively through iyw-claw's existing user-memory system. Never
create a second durable user-memory database.

## Layered Memory Contract

Use a low-noise five-layer funnel:

| Layer | Meaning | Action |
|---|---|---|
| M0 Session context | Temporary instructions and task state | Use now; never persist |
| M1 Candidate memory | Explicit reusable preference, fact, goal, constraint, or correction | Call `propose_user_memory` |
| M2 Confirmed memory | Eligible information the user explicitly asks to remember | Call `append_user_memory` |
| M3 User profile | Stable synthesis of confirmed memories | Leave to iyw-claw; never edit `user-profile.md` |
| M4 Interaction principles | Highly stable communication and long-term boundaries | Leave to iyw-claw; never edit `user-soul.md` |

User memory remains owned by iyw-claw. Never edit `user-memory.md`,
`user-profile.md`, or `user-soul.md` directly or create parallel storage.
Operational reflections are separate and are not a memory layer.

## Tool Name Resolution

`append_user_memory` and `propose_user_memory` are served by the
`iyw-claw-mcp` MCP server, so different agents expose them under different
prefixed names (for example `iyw-claw-mcp__append_user_memory` or
`mcp__iyw-claw-mcp__append_user_memory`). Before calling, find the tool in
your own tool list whose name **ends with** the bare name used in this skill
and call that exact listed name. Calling a bare name that is not in your tool
list fails (for example Codex returns `unsupported call`). If no tool with
that suffix is listed, the memory feature is unavailable for this session.

If the relevant host memory tool is unavailable, continue without persistence
and state that no durable memory was changed. Do not fall back to shell or
file-edit tools.

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

## Proactive Routing

When an eligible signal appears naturally in the user's message:

1. Filter prohibited sensitive, secret, inferred, and third-party content.
2. Keep one-time instructions and temporary state in M0.
3. Route an explicit remember request to M2 with `append_user_memory`.
4. Route another explicit reusable signal to M1 with
   `propose_user_memory` using `correction`, `preference`, or `fact`.
5. Deduplicate repeated signals in the visible context; repetition never
   grants permission to promote a candidate.
6. Stay silent when there is no reusable signal. Do not interrogate the user
   to build a profile or interrupt the active task to manufacture memory.

| Signal | Layer and action |
|---|---|
| "Remember that I prefer..." | M2: `append_user_memory` |
| Explicit reusable correction | M1: `propose_user_memory` with `correction` |
| Explicit reusable preference, goal, or constraint | M1: `propose_user_memory` with `preference` |
| Explicit reusable fact | M1: `propose_user_memory` with `fact` |
| Repeated signal in visible context | Deduplicate in M1; never auto-promote |
| One-time instruction | M0: follow now; do not persist |
| Silence, tone, behavior, or inferred trait | Do not learn from it |
| Agent execution lesson | Operational reflection only |

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
