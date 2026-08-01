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

1. Do not retry alternate names, guess prefixes, or claim the memory was
   saved via MCP.
2. Return the stable error to the user with the reason and a retry
   suggestion. Never use `shell_command` to write user memory files and
   never fall back to a hardcoded path — the host memory service resolves
   the real root.
3. If the user explicitly confirms, the User Memory settings page or the
   message **Remember** action writes through the host service.

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
