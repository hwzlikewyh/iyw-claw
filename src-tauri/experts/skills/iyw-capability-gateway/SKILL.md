---
name: iyw-capability-gateway
short-description: Route concrete iyw-claw host work through the live capability catalog.
description: >-
  Use proactively when a concrete task needs iyw-claw host state or action:
  memory or self-learning, session/profile/history, final artifacts, managed
  browser or public web evidence, audio transcription, image understanding or
  display, channels and messages, scheduled automation, user interaction, or
  delegated work. First load the matching gateway reference, then use one
  complete search/read/invoke trio against the current catalog. Prefer a direct
  tool or domain Skill when it fully owns the task; when a required input or
  user decision is unclear, ask through the interaction capability before
  acting; never guess IDs, arguments, paths, URLs, or schemas.
routing:
  capability: iyw-claw host routing through live capabilities
  coreTriggers: [host action, memory, self-learning, session, profile, history, artifact, browser, web, internet, audio, transcription, image understanding, channel, message, automation, scheduled task, feedback, question, clarification, ambiguous requirement, needs decision, 需求不清, 需要选择, delegation]
  exclusions: [trivial request, self-contained explanation, direct tool fully covers the task, incomplete gateway trio]
  aliases: [iyw gateway, host capability, capability catalog, 主机能力, 能力网关]
  invocation: Load the matching reference, search the live catalog, read one best match, and invoke its exact current schema.
---

# IYW Capability Gateway

This Skill is an active routing gate, not a static tool list. The host catalog is
authoritative for current capability IDs, schemas, required inputs, availability,
permissions, and schema digests.

## Load References Actively

When a trigger below is present, **load the named reference before searching and
follow its workflow**. Do not treat the reference as optional background reading.

| Task signal | Load first |
| --- | --- |
| Session, profile, history, interaction, or plugin capability | [capability-families.md](references/capability-families.md) |
| Unclear requirement, missing decision, or multiple reasonable interpretations | [capability-families.md](references/capability-families.md) |
| Final file, directory, URL, HTML/Markdown delivery, or image references in a document | [artifact-delivery.md](references/artifact-delivery.md) |
| Channel discovery, targets, messages, credentials, QR authorization, or connection state | [channel-operations.md](references/channel-operations.md) |
| Scheduled task project selection, cron, create, update, pause, or delete | [automation.md](references/automation.md) |
| Independent subtask, parallel Agent, task ID, wait, or cancellation | [delegation.md](references/delegation.md) |
| Web page, public web data, browser interaction, screenshot, visual page, audio, transcription, or image understanding | [browser-and-media.md](references/browser-and-media.md) |
| Prior decisions, preferences, repeated workflows, memory, learning, correction, candidate, or memory repair | [memory-and-learning.md](references/memory-and-learning.md) |
| Research, comparison, investigation, current web evidence, or cited report | [research-workflow.md](references/research-workflow.md), plus [browser-and-media.md](references/browser-and-media.md) for browser work |
| Platform, URL, social discussion, GitHub, video, podcast, RSS, finance, or login-backed source | [internet-routing.md](references/internet-routing.md) |
| Unsure which family or how to call the trio | [tool-usage.md](references/tool-usage.md) |

## Route Proactively

1. Use an exact visible direct tool when it fully satisfies the current
   sub-goal. Otherwise use the domain Skill that owns the business workflow:
   `agent-browser`, `iyw-image-workflows`, `imagegen`, `wecom-unified`,
   `open-computer-use`, `skill-creator`, `skill-installer`, `plugin-creator`,
   `writing-plans`, or `executing-plans`.
   For any IYW product, material, pattern, trend, knowledge, commerce,
   product-kit, model-scene, try-on, 3D, video, background, line-art, color,
   or image-tool request, load `iyw-image-workflows` first. It owns tool
   selection, upload/check, dynamic settings, prompt templates, and task
   contracts. When no dedicated tool is named, prefer `extend` for a baseline
   series/trend task, `mix` for 2-10 inputs with a fusion goal, `variation` for
   one-image bounded changes, and fission for pure text creation. Do not route
   those requests to `imagegen` merely because they contain the words "generate image".
2. Use this gateway for the remaining iyw-claw host sub-goal: current session
   state, user profile, historical task lookup, memory, artifacts, browser
   host actions, audio, image display/understanding, channels, automation,
   interaction, delegation, or a live plugin capability.
3. Inspect the actual callable surface and select one complete trio of
   `search_iyw_capabilities`, `read_iyw_capability`, and
   `invoke_iyw_capability`. Prefer the unique visible
   `iyw-claw-builtin-*` trio; if the trio is incomplete or ambiguous, stop and
   use an actually visible direct route.

## Mandatory Gateway Sequence

1. Search with 2-5 action/object terms in Chinese or English, such as
   `查询 历史 记忆`, `读取 网页`, `会议 音频 转写`, or `send channel message`.
2. Treat results as current session evidence. Read the best plausible stable
   `capability_id`; read at most one same-result alternative if needed.
3. Invoke only the ID returned by the current search and only with the schema
   returned by the current read. Ask for a missing primary object instead of
   guessing it.
4. Verify the business result, status, and any required follow-up. Preserve an
   `iyw_delivery_receipt` exactly as top-level `delivery_ack` on the next real
   invocation.

An empty result, unavailable capability, malformed output, timeout, unknown ID,
schema rejection, or two non-matching reads ends gateway use for this turn. Do
not switch namespaces, invent names, cycle locators, or replay stale arguments.

## Image Workflow Bridge

For a combined image task, split the work into two layers:

1. Let `iyw-image-workflows` select the website-backed operation and settings.
   Its scenario playbook is based on the current `ai.iyw.cn` tool pages and
   must be read when the request names a specific tool or has a product image.
2. Use this gateway only for host-owned work around that operation: retrieve
   relevant memory, inspect or display an image, present a browser page, and
   register final artifacts. Search the live catalog for the exact capability
   IDs and schemas before each host action.

This ordering is the default automatic priority. A user naming `imagegen`, GPT
Image, or another exact tool still overrides it for that sub-goal; remaining
IYW or host sub-goals are routed independently. Website point totals, model
choices, and channel settings are live values and must never be copied into a
static gateway rule.

## Memory Gate

For any relevant memory operation, load `memory-and-learning.md` and run the
current-turn `read_memory_policy` preflight before the first other memory call.
For substantive coding, debugging, configuration, research, or multi-step work,
proactively perform one bounded recall unless the request is clearly
self-contained. Use the returned `matched`, `no_evidence`, or `unavailable`
state honestly; do not claim that no history exists from a timeout.

## Do Not Bypass the Host

Never edit host-owned memory documents with shell tools, expose credentials or
provider IDs, use arbitrary browser paths, register internal files as artifacts,
or treat this document as a replacement for the live catalog. The host owns
authorization, locking, idempotency, confirmation, cancellation, persistence,
and result semantics.
