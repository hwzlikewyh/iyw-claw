# Self-Improving Layered Memory Implementation Plan

> **For agentic workers:** Use `executing-plans` to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `self-improving` proactively recognize explicit reusable user information and route it through a five-layer, review-first memory model.

**Architecture:** Keep iyw-claw as the only durable user-memory authority. Expand the skill trigger metadata and route volunteered information through M0 session context, M1 proposals, M2 confirmed memory, and host-owned M3/M4 synthesis, while keeping Agent operational reflections separate.

**Tech Stack:** Markdown skill instructions, YAML frontmatter, iyw-claw host memory tools, PowerShell validation, Python skill validator.

## Global Constraints

- Never edit `~/.iyw-claw/user-memory.md`, `user-profile.md`, or `user-soul.md` directly.
- Never create a second user-memory database under `~/.iyw-claw/self-improving/`.
- Never persist secrets, sensitive attributes, third-party information, behavioral inference, or one-time task state.
- Route non-sensitive explicit reusable information to `propose_user_memory` unless the user explicitly asks to remember it.
- Keep heartbeat and operational reflections isolated from all user-memory layers.
- Do not modify `self-improving/_meta.json` or any heartbeat file.
- Do not commit or push without separate user authorization.

---

### Task 1: Expand Automatic Triggering and Primary Routing

**Files:**
- Modify: `self-improving/SKILL.md:1`

**Interfaces:**
- Consumes: iyw-claw host tools `append_user_memory` and `propose_user_memory`.
- Produces: The authoritative trigger description, M0-M4 layer definitions, and routing rules referenced by the bundled memory documents.

- [x] **Step 1: Run the trigger contract check and verify the current skill fails**

Run:

```powershell
$skill = Get-Content -Raw self-improving/SKILL.md
$required = @(
  'M0', 'M1', 'M2', 'M3', 'M4',
  'proactively', 'first-person', 'propose_user_memory',
  'append_user_memory', 'do not interrogate'
)
$missing = $required | Where-Object { $skill -notmatch [regex]::Escape($_) }
if ($missing) { throw "Missing trigger contract: $($missing -join ', ')" }
```

Expected: FAIL because the existing skill does not define M0-M4 or proactive first-person trigger signals.

- [x] **Step 2: Replace the frontmatter description with the proactive trigger contract**

Use exactly two frontmatter fields and set the description to:

```yaml
---
name: self-improving
description: MUST use proactively whenever the user explicitly asks to remember something; states a first-person durable preference, fact, long-term goal, or recurring constraint; corrects reusable Agent behavior; repeats a durable signal in the visible context; or when the Agent should record a non-user operational reflection after significant work. Route volunteered non-sensitive information through review-first layered memory. Do not trigger persistence from silence, behavior, tone, inferred traits, secrets, third-party information, or one-time instructions.
---
```

- [x] **Step 3: Replace `Memory Contract` and `Signal Routing` with the five-layer routing model**

Keep the existing iyw-claw ownership statement, operational reflection workflow, and hard boundaries. Add this primary contract before the operational-state section:

```markdown
## Layered Memory Contract

Use a low-noise five-layer funnel:

| Layer | Meaning | Action |
|---|---|---|
| M0 Session context | Temporary instructions and task state | Use now; never persist |
| M1 Candidate memory | Explicit reusable preference, fact, goal, constraint, or correction | Call `propose_user_memory` |
| M2 Confirmed memory | Eligible information the user explicitly asks to remember | Call `append_user_memory` |
| M3 User profile | Stable synthesis of confirmed memories | Leave to iyw-claw; never edit `user-profile.md` |
| M4 Interaction principles | Highly stable communication and long-term boundaries | Leave to iyw-claw; never edit `user-soul.md` |

User memory remains owned by iyw-claw. Never edit its durable files directly
or create parallel storage. Operational reflections are separate and are not a
memory layer.

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
```

Replace the signal table with:

```markdown
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
```

- [x] **Step 4: Validate frontmatter and primary trigger content**

Run:

```powershell
$env:PYTHONUTF8 = '1'
uv run --with pyyaml python skill-creator/scripts/quick_validate.py self-improving
```

Expected: `Skill is valid!`

Run the Step 1 contract check again.

Expected: PASS with no output.

- [x] **Step 5: Review the Task 1 diff without committing**

Run:

```powershell
git diff --check -- self-improving/SKILL.md
git diff -- self-improving/SKILL.md
```

Expected: no whitespace errors; diff contains only the trigger and routing changes described above.

---

### Task 2: Align Layer, Learning, Operations, and Privacy References

**Files:**
- Modify: `self-improving/memory.md:1`
- Modify: `self-improving/learning.md:1`
- Modify: `self-improving/operations.md:1`
- Modify: `self-improving/boundaries.md:1`

**Interfaces:**
- Consumes: The M0-M4 contract and routing rules defined in Task 1.
- Produces: Consistent reference guidance for classification, promotion, conflict handling, privacy, and unavailable tools.

- [x] **Step 1: Run the reference consistency check and verify it fails before edits**

Run:

```powershell
$paths = @(
  'self-improving/memory.md',
  'self-improving/learning.md',
  'self-improving/operations.md',
  'self-improving/boundaries.md'
)
$text = ($paths | ForEach-Object { Get-Content -Raw $_ }) -join "`n"
$required = @('M0', 'M1', 'M2', 'M3', 'M4', 'auto-promote', 'interrogate')
$missing = $required | Where-Object { $text -notmatch [regex]::Escape($_) }
if ($missing) { throw "Missing reference contract: $($missing -join ', ')" }
```

Expected: FAIL because the existing references do not define the layer and promotion vocabulary.

- [x] **Step 2: Replace `memory.md` with the layer ownership reference**

Use this content:

```markdown
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
```

- [x] **Step 3: Replace `learning.md` with the classification and promotion reference**

Use this content:

```markdown
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
```

- [x] **Step 4: Update `operations.md` with layer-aware operations and failure handling**

Replace the durable-user-memory table with:

```markdown
| Request or signal | Layer and action |
|---|---|
| One-time instruction | M0: use now and do not persist |
| Explicit reusable correction/preference/goal/constraint/fact | M1: call `propose_user_memory` |
| Explicit remember request | M2: call `append_user_memory` |
| Approved candidate | M2: let the host approval flow persist it |
| Profile or interaction-principle synthesis | M3/M4: leave to iyw-claw |
| Show, review, forget, or export | Use supported host UI or operations |
```

Add these rules after the table:

```markdown
Repeated signals remain candidates until the user approves them. When current
input conflicts with older memory, follow the current instruction and propose
an update. Never edit or rewrite the storage files directly.

If a required host memory tool is unavailable, continue the active task
without persistence and say that durable memory was not changed. Do not fall
back to shell or file-edit tools.
```

- [x] **Step 5: Expand `boundaries.md` with collection and promotion limits**

After the existing prohibited-data paragraph, add:

```markdown
Do not interrogate the user to complete a profile. Do not infer memory from
silence, tone, emotion, browsing, clicks, access patterns, or repeated Agent
guesses. Only information the user explicitly volunteers is eligible.

Keep one-time information in M0. Put other eligible reusable information in
M1 for review. Never auto-promote M1 because of repetition, confidence, or
convenience. M3 and M4 are host-owned syntheses and must never be written by
this skill.

Only explicit durable user corrections, preferences, goals, recurring
constraints, and facts are eligible. Repository details, temporary progress,
and Agent private reasoning are ineligible.
```

- [x] **Step 6: Run reference and privacy consistency checks**

Run the Step 1 reference check again.

Expected: PASS with no output.

Run:

```powershell
rg -n "Never read|Never modify|user-memory|user facts" `
  self-improving/heartbeat-rules.md `
  self-improving/HEARTBEAT.md `
  self-improving/openclaw-heartbeat.md
```

Expected: heartbeat instructions still limit review to operational state and prohibit user-memory processing.

- [x] **Step 7: Run final skill validation and diff review**

Run:

```powershell
$env:PYTHONUTF8 = '1'
uv run --with pyyaml python skill-creator/scripts/quick_validate.py self-improving
git diff --check -- self-improving docs/superpowers
git diff --stat -- self-improving docs/superpowers
git status --short
```

Expected: validator reports `Skill is valid!`; no whitespace errors; only the approved design, implementation plan, and five target skill files are modified or added. Existing unrelated untracked files remain unstaged.
