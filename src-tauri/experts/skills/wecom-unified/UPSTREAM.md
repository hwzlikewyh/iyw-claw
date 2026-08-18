# Upstream

- Repository: `https://github.com/WeComTeam/wecom-unified`
- Commit: `2620033fdb2721fb192ac06a6a0510987035d203`
- Imported path: `skills/wecom-unified`
- License: MIT, retained in `LICENSE`

## Host adaptation

`SKILL.md` does not run `npm install -g @wecom/cli` when the CLI is missing or
outdated. iyw-claw installs the pinned CLI in its private application data
directory and places an application-owned, fail-closed launcher first on Agent
and ACP terminal `PATH`.
