# Durable Memory Notice

This bundled file is not runtime memory and must never be copied into a user
directory. iyw-claw stores durable memory in `~/.iyw-claw/user-memory.md` and
manages it through `append_user_memory`, `propose_user_memory`, and the User
Memory settings UI.

Agent-authored operational reflections belong only in
`~/.iyw-claw/self-improving/reflections.md` and must not contain user facts,
preferences, or secrets.
