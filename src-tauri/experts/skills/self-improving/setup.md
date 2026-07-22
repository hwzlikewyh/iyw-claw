# Self-Improving Operational Setup

iyw-claw creates and manages durable user-memory files. This setup initializes
only Agent-authored operational state.

## Windows PowerShell

```powershell
$stateDir = Join-Path $env:USERPROFILE ".iyw-claw\self-improving"
New-Item -ItemType Directory -Force $stateDir | Out-Null
```

Create `heartbeat-state.md` from the bundled
[heartbeat-state.md](heartbeat-state.md) template and create `reflections.md`
with this header when either file is missing:

```markdown
# Self-Improving Reflections

Operational process lessons only. No user facts, preferences, or secrets.
```

## POSIX Shell

```sh
mkdir -p "$HOME/.iyw-claw/self-improving"
```

Do not create `memory.md`, `corrections.md`, `projects/`, `domains/`, or
`archive/`. Do not modify AGENTS.md, SOUL.md, or workspace HEARTBEAT.md during
setup. Durable learning must use `append_user_memory` or
`propose_user_memory`.

## Verification

Confirm the operational directory contains only non-sensitive reflection and
heartbeat state. Confirm user-memory settings remain controlled by iyw-claw.
