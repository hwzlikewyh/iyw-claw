---
name: iyw-capability-gateway
short-description: Route iyw-claw host actions through the live capability catalog.
description: Use when a concrete task needs iyw-claw host state or action and one complete gateway trio is visible. Search the live catalog, read the best match, and invoke its exact current schema. Skip trivial requests and never guess IDs or arguments.
routing:
  capability: iyw-claw host routing
  coreTriggers: [host action, memory, artifact, browser, channel, automation]
  exclusions: [trivial request, direct tool fits, incomplete gateway]
  aliases: [iyw gateway, host capability, 主机能力]
  invocation: Search, read best match, invoke exact current ID and schema.
---

# IYW Capability Gateway

Use this Skill as a routing gate, not as a static tool catalog. The current
callable surface and live capability catalog are authoritative for tool names,
stable IDs, schemas, required inputs, availability, and schema digests.

## When to Search

Search the gateway when a concrete goal needs iyw-claw host state or action and
no visible direct tool already completes that subgoal. Typical categories:

| Category | Search when the task needs |
| --- | --- |
| Memory | recall of prior decisions/preferences, or durable memory write |
| Session | current session state or account profile |
| Artifacts | final files, directories, or public URLs delivered to this conversation |
| Image | image understanding/display through host tools |
| Browser | managed browser navigation or interaction |
| Audio | transcription or transcription result lookup |
| Interaction | user feedback or a required user decision |
| Delegation | a bounded task another Agent can execute independently |
| Channels | configured channel discovery, message history, or sending |
| Automation | scheduled-task listing, creation, update, or deletion |

Do not search for greetings, ordinary explanations, self-contained translation,
one-line commands, current-turn-only context, or to enumerate tools. If the
required primary object is missing, ask for it before searching.

## Gateway Gate

The logical roles are `search_iyw_capabilities`, `read_iyw_capability`, and
`invoke_iyw_capability`; they are not automatically top-level tools. Use one
complete trio on one current callable surface only:

1. Prefer the unique visible `iyw-claw-builtin-*` trio.
2. Otherwise use a complete bare trio.
3. Otherwise use the only remaining complete trio in one nested registry.

If a tier has multiple trios, or any role is missing, do not use the gateway.
For nested tools, invoke them only through their owning orchestration tool.

## Progressive Disclosure

1. Search with 2-5 discriminating action/object terms in Chinese or English,
   such as `查询 历史 记忆`, `提交 成果 文件`, or `send channel message`.
2. Treat returned summaries, aliases, `when_to_use`, status, required inputs,
   and schema digest as current catalog evidence.
3. Read the best matching result using its exact stable ID. Read at most one
   other candidate from the same result set if the first does not fit.
4. Invoke only an available ID returned by the current search and only with
   arguments matching the current read schema.

An empty result, no plausible match, two non-matching reads, malformed output,
timeout, unknown ID, unavailable capability, or routing error ends gateway use
for the turn. Do not retry through another namespace, invent a tool name, or
guess an argument. One search retry is allowed only after an exhausted result
set, using a close synonym.

## Memory

Recall is task-sensitive, not mandatory for every turn.

- When the task refers to previous work, prior decisions, user preferences,
  repeated workflows, or historical context, search with intent such as
  `recall memory history`, then read and invoke the exact current capability.
- Skip recall for simple self-contained requests and when the user supplied all
  relevant context. `no_evidence` means no matching evidence, not that the user
  lacks the fact; `unavailable` is a routing limitation.
- When the user explicitly asks to remember a durable fact, preference, or
  correction, search with intent such as `remember confirmed memory`.
- For a conservative reusable fact or correction that may be valuable but was
  not explicitly confirmed, search with intent such as `propose memory`.
- Never store passwords, tokens, cookies, private keys, full credentials,
  transient task state, or speculative claims.
- Never edit `user-memory.md`, `user-profile.md`, or `user-soul.md` directly.
  Host memory capabilities own persistence, locking, candidate lifecycle, and
  recall context.

## Images

- IYW product, material, trend, knowledge, and commerce workflows: use the
  installed `iyw-image-workflows` Skill first.
- Free editing, GPT Image requests, or GPT Image-specific parameters: use the
  installed `imagegen` Skill first. Do not ask the user to separately specify
  GPT when the request already says GPT Image.
- Understanding an existing image is `analyze_image`; displaying a result is
  `show_image`. Neither is an image-generation route.

Follow the selected image Skill's execution contract. Do not guess image API
endpoints or payloads in this gateway.

## Final Artifacts

If the task produces a final user-facing file, directory, or public URL, it
must be registered in the current conversation Artifacts before completion is
claimed. Prefer a directly visible `present_task_files`; otherwise discover it
through this gateway. Submit all final items together when possible.

Do not register source, configuration, tests, migrations, build output, caches,
logs, temporary files, or internal work unless the user explicitly requested
that exact item as the deliverable. If the artifact route is unavailable or
rejects an item, report that delivery was not completed. A Markdown preview or
an ordinary URL alone is not proof of Artifact registration.

## Other Categories

- Current account identity belongs to the session/profile category, not memory.
- Browser references expire after navigation, a route change, popup, material
  DOM update, or write. For a stale reference or locator failure, make only one
  recovery attempt for the same action: take a fresh snapshot and use one new
  reference or revised locator. Do not extend the budget by cycling locators.
  For runtime/session/daemon/observer unavailability, a crashed tab, or timeout,
  inspect state once and switch to `opencli-browser` only when that Skill and
  the `opencli` command are available. Read its `SKILL.md`, run `opencli doctor`,
  then use one stable session with `bind`/`open`, `state`/`find`, an action, and
  explicit verification. Otherwise report the missing prerequisite.
- Destructive automation, channel, browser, or delegation operations require
  an exact target and the confirmation rules returned by `read`.
- For long work, use a visible feedback capability or discover one at sensible
  checkpoints; use a question capability only for a necessary unresolved input.

## Delivery Receipts

If an invocation returns `iyw_delivery_receipt`, preserve it exactly. On the
next real invocation, send it as the top-level `delivery_ack`, never inside
business arguments. Do not fabricate an invocation just to acknowledge a
receipt.
