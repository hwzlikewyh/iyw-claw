# Learning Signals

## Eligible User Signals

- Explicit correction: propose with signal `correction`.
- Explicit reusable preference: propose with signal `preference`.
- Explicit reusable fact: propose with signal `fact`.
- Explicit remember request for a durable fact/preference: append directly.

## Ineligible Signals

- Silence, tone, inferred emotion, or guessed intent.
- One-time instructions and temporary task progress.
- Repository details that belong in project documentation.
- Secrets, sensitive traits, and third-party information.
- Agent self-evaluation or private reasoning.

Agent execution lessons may be written only as non-sensitive operational
entries in `~/.iyw-claw/self-improving/reflections.md`. They are not user
memory and must not influence unrelated tasks as if the user confirmed them.
