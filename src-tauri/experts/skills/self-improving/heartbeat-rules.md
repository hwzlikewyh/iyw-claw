# Self-Improving Heartbeat Rules

Heartbeat reviews only Agent-authored operational state under
`~/.iyw-claw/self-improving/`.

1. Ensure `heartbeat-state.md` exists.
2. Read its `last_reviewed_reflection_at` value.
3. Check whether `reflections.md` contains newer entries.
4. If nothing changed, set `last_heartbeat_result: HEARTBEAT_OK` and stop.
5. If entries changed, remove exact duplicates and compact verbose wording
   without changing meaning.
6. Update timestamps and a concise non-sensitive action note.

Never read, reorganize, summarize, or delete iyw-claw user-memory documents as
part of heartbeat. Never modify files outside
`~/.iyw-claw/self-improving/`. Never turn Agent reflections into user facts or
preferences.
