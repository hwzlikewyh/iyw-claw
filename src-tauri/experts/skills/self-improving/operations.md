# Memory Operations

## Durable User Memory

Use iyw-claw host tools and settings as the only authority.

| Request | Action |
|---|---|
| Remember a durable fact/preference | Call `append_user_memory` |
| Record a reusable correction/preference/fact for review | Call `propose_user_memory` |
| Show or review memory | Direct the user to iyw-claw User Memory settings or use an available read surface |
| Forget/delete memory | Use the supported user-memory delete/resolve UI; confirm the exact target |
| Export memory | Use iyw-claw backup/export behavior |

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
