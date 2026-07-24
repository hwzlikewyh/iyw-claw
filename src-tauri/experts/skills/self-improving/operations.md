# Memory Operations

## Layered User Memory

Use iyw-claw host tools and settings as the only authority.

| Request or signal | Layer and action |
|---|---|
| One-time instruction | M0: use now and do not persist |
| Explicit reusable correction/preference/goal/constraint/fact | M1: call `propose_user_memory` |
| Explicit remember request | M2: call `append_user_memory` |
| Approved candidate | M2: let the host approval flow persist it |
| Profile or interaction-principle synthesis | M3/M4: leave to iyw-claw |
| Show, review, forget, or export | Use supported host UI or operations |

Repeated signals remain candidates until the user approves them. When current
input conflicts with older memory, follow the current instruction and propose
an update. Never edit or rewrite the storage files directly.

If a required host memory tool is unavailable, continue the active task
without persistence and say that durable memory was not changed. Do not fall
back to shell or file-edit tools.

Never use shell commands to edit `~/.iyw-claw/user-memory.md`,
`user-profile.md`, or `user-soul.md`.

## Operational Reflections

Append short, non-sensitive process lessons to
`~/.iyw-claw/self-improving/reflections.md`. One entry should contain date,
task context, observable issue, and a next-time action. Do not copy user memory
into this file.

Heartbeat maintenance may update
`~/.iyw-claw/self-improving/heartbeat-state.md`; see
[heartbeat-rules.md](heartbeat-rules.md).

## Conflicts

Current user and project instructions always override older memory. When a
durable preference appears stale or contradictory, submit a new proposal or
ask the user; do not silently rewrite the memory files.
