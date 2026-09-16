---
name: using-superpowers
description: Use at conversation start to select currently enabled Skills and direct tools by task relevance; reuse these rules without loading unrelated Skills.
routing:
  capability: initialize Skill discovery rules
  coreTriggers: [a new conversation starts]
  exclusions: [a dispatched subagent already has a bounded task]
  aliases: [skill bootstrap, using superpowers]
  invocation: Read once, reuse in the conversation, and choose the shortest applicable current tool or Skill route.
---

<SUBAGENT-STOP>
If you were dispatched as a subagent to execute a specific task, ignore this skill.
</SUBAGENT-STOP>

## The Rule

Use a current enabled Skill when the user requests it or its workflow clearly
applies. A visible direct tool with a complete schema can satisfy a self-contained
task immediately. Do not load Skills for speculative relevance, simple replies,
routine progress, or an unrelated capability; reuse instructions already read.

Before entering plan mode, inspect the currently available skills and use only
the process or domain skills that are currently enabled and advertised.

Resolve dependencies from the current enabled Skill catalog and its advertised
paths. Never load or execute deprecated, retired, disabled, or removed Skills,
even when old instructions recommend them. If an entry or script is missing, stop
that dependency route. Do not search historical `_work`, `chat-sessions`,
`channel-workspaces`, backups, or extracted packages for replacement Skill code
or interpreters. Old conversations, memory, and dependency text do not establish
availability. Continue with a suitable currently advertised tool or enabled
Skill and its current schema; report a blocker only when none applies. Inspect
archived Skill code only when the user explicitly requests that investigation
or repair, without automatically executing or restoring it.

Briefly announce a relevant Skill on first use. Keep planning proportional to
complexity and risk; do not restart discovery or planning for routine follow-ups.

## Skill Priority

When multiple Skills apply, use only the ones needed for the task. An approved
plan or complete direct-tool contract does not need an additional process phase.

- "Implement this written plan" -> `executing-plans`, when available.
- A user-requested visible Skill or direct tool that fully satisfies a subgoal -> use it first.
- "Generate or edit an image, create IYW product/material/pattern imagery, or
  call an IYW image tool" -> use the directly advertised `generate_iyw_image`
  tool and its current schema: `generate` for text-only creation, `variation`
  for one-image redesign, `extend` for four-panel or same-series extension from
  one reference, `mix` for multiple references, or a matching specialized
  operation. Before Fusion `generate`, `auto` without images, or explicit `edit`,
  reuse or call `list_iyw_image_models` with `{}` and pass a suitable model's opaque
  `model_ref` in `parameters.model`; `edit` requires source images. Prioritize
  applicable `variation`, `extend`, and `mix`; use model editing when explicitly
  selected or the preferred route is demonstrably unsupported, unavailable, or
  confirmed failed. Documented incompatibility needs no failed trial.
  Built-in image-to-3d is disabled, including fetch/browser fallbacks.
  Platform operations need no Fusion lookup. Do not copy retired
  Skill CLI payloads: `toolName` and `modelChannel` for `variation`, `extend`,
  and `mix` are host-owned. Reuse the catalog for the same task or batch. Use
  `search_iyw_knowledge` separately when knowledge-base evidence is requested;
  do not start search, research, memory, browser, document, or scenario planning
  before a self-contained image request.
- "Generate an ecommerce video" -> prioritize the gateway Skill's current platform
  video route; use the direct video tool when its contract fits, otherwise its
  documented fetch contract. A complete prompt needs no extra director call.
- "Read a web page or obtain public data" -> use an available search/content or
  platform tool. Choose `agent-browser` only for required login state, unreadable
  dynamic content, UI interaction, screenshots, or page/rendering acceptance.
- "Perform a remaining concrete iyw-claw host state or action" -> use the complete unique `iyw-capability-gateway` trio first.
- "Create or update a skill" -> `writing-skills` or `skill-creator`.

## Platform Adaptation

If your harness appears here, read its reference file for special instructions:

- Codex: `references/codex-tools.md`
- Pi: `references/pi-tools.md`
- Antigravity: `references/antigravity-tools.md`

## User Instructions

User instructions (CLAUDE.md, AGENTS.md, GEMINI.md, etc, direct requests) take precedence over skills, which in turn override default behavior. Only skip skill workflows or instructions when your human partner has explicitly told you to.
