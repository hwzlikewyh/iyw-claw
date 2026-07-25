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

## Route Preflight And Fallback

For `append_user_memory` or `propose_user_memory`, inspect the current tool list
and accept an exact bare-name match or a suffix match at a separator boundary
such as `__`, `_`, `.`, `/`, or `:`. Require exactly one match and call that
complete listed name. Zero matches means the route is unavailable; multiple
matches are ambiguous. In either case, never guess a prefix or call an unlisted
bare name.

On `unsupported call`, route unavailable, or a structured memory error:

1. Do not retry alternate names or claim the memory was saved.
2. Continue the user's active task and say no durable memory change was
   confirmed.
3. Point the user to the host **Memory** (brain) action beside the source
   message. It bypasses MCP and uses iyw-claw's existing memory service.
4. Never use shell commands or direct file edits as a fallback.

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
