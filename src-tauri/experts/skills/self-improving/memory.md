# Layered User Memory

Use iyw-claw's existing user-memory system as the only durable authority.

| Layer | Content | Authority |
|---|---|---|
| M0 | Current-session instructions and task state | Conversation context only |
| M1 | Explicit reusable candidate facts, preferences, goals, constraints, and corrections | `propose_user_memory` review flow |
| M2 | Eligible information the user explicitly asks to remember or approves | `append_user_memory` or host approval flow |
| M3 | Stable profile synthesized from confirmed memory | iyw-claw `user-profile.md` |
| M4 | Highly stable interaction principles | iyw-claw `user-soul.md` |

Do not copy this file into a user directory. Do not edit the M2-M4 storage
files directly. M1 can advance to M2 only with user approval; repetition is
not approval. Agent operational reflections are separate from M0-M4 and must
not contain user information.
