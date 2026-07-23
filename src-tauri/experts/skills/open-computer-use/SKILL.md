---
name: open-computer-use
description: Use when operating or troubleshooting the Open Computer Use MCP service from iyw-claw or a standalone Agent runtime on macOS, Linux, or Windows.
---

# Open Computer Use

## Overview

Open Computer Use is an open-source Computer Use service exposed through MCP. It lets compatible AI agents operate local desktop apps on macOS, Linux, and Windows through tools such as `list_apps`, `get_app_state`, `click`, `type_text`, `press_key`, `scroll`, `drag`, and `set_value`.

Use this skill when a user wants to install, configure, verify, troubleshoot, or operate Open Computer Use from an agent runtime such as Codex, Claude Code, Gemini CLI, opencode, or another MCP client.

## iyw-claw Managed Setup

When this Skill is available inside iyw-claw, the application owns the private
runtime and MCP registration for every enabled Agent.

- Do not run `npm i -g`, an `install-*-mcp` command, or edit an Agent's MCP
  configuration.
- Use the exposed Computer Use tools directly.
- If the tools are unavailable, ask the user to enable Computer Use in
  Settings and start a new Agent session after installation finishes.
- Treat a missing tool as an iyw-claw setup or session-refresh issue, not as
  permission to modify `~/.codex/config.toml` or another global config.

## Standalone Setup

Use these commands only when the Agent is running outside iyw-claw or the user
explicitly requests a standalone installation.

### Install

Install the CLI:

```sh
npm i -g open-computer-use
```

On macOS, run it once and grant Accessibility and Screen Recording permissions:

```sh
open-computer-use
```

Windows and Linux do not require the macOS permission step.

### MCP Setup

Install into Codex:

```sh
open-computer-use install-codex-mcp
```

Or configure any MCP client manually:

```json
{
  "mcpServers": {
    "open-computer-use": {
      "command": "open-computer-use",
      "args": ["mcp"]
    }
  }
}
```

### Other Agent Integrations

Open Computer Use also includes installer commands for common agent runtimes:

```sh
open-computer-use install-codex-plugin
open-computer-use install-claude-mcp
open-computer-use install-gemini-mcp
open-computer-use install-opencode-mcp
```

## Direct CLI Calls

Use the CLI to call one tool:

```sh
open-computer-use call list_apps
open-computer-use call get_app_state --args '{"app":"TextEdit"}'
```

Run a sequence in one process so element indexes can be reused:

```sh
open-computer-use call --calls '[{"tool":"get_app_state","args":{"app":"TextEdit"}},{"tool":"press_key","args":{"app":"TextEdit","key":"Return"}}]'
```

Check setup and permissions:

```sh
open-computer-use doctor
```

## Operating Rules

- Treat Computer Use as control over the user's real desktop session.
- Ask before actions that send messages, submit forms, delete data, install software, change system settings, make purchases, or expose sensitive local information.
- Prefer accessibility element indexes from `get_app_state` over coordinate clicks when available.
- Re-read app state after important actions before continuing.
- On macOS, if tools fail due to permissions, ask the user to grant Accessibility and Screen Recording for the Open Computer Use app or runtime process.

## Project Links

- Repository: https://github.com/iFurySt/open-codex-computer-use
- npm package: https://www.npmjs.com/package/open-computer-use
