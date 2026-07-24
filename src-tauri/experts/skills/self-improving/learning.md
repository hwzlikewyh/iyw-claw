# Learning Signals

## Classification Order

1. Reject secrets, sensitive information, inferred traits, behavioral signals,
   and third-party personal information.
2. Keep temporary and one-time instructions in M0.
3. Send an eligible explicit remember request to M2 with
   `append_user_memory`.
4. Send another explicit reusable correction, preference, goal, constraint,
   or fact to M1 with `propose_user_memory`.
5. Do nothing when there is no eligible signal. Do not interrogate the user to
   collect profile data.

## Candidate Signals

- Explicit correction: propose with signal `correction`.
- Explicit reusable preference, goal, or recurring constraint: propose with
  signal `preference`.
- Explicit reusable fact: propose with signal `fact`.
- Explicit remember request for an eligible durable fact or preference: append
  directly as confirmed memory.

## Promotion and Deduplication

- Promote M1 to M2 only after user approval.
- Deduplicate equivalent signals in the visible context.
- Never auto-promote because a signal repeats or appears likely.
- For a conflict, follow the current instruction and propose an update; never
  silently rewrite durable memory.

Agent execution lessons belong only in operational reflections. They are not
user memory and cannot be promoted into M1-M4.
