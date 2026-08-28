---
name: iyw-capability-gateway
short-description: Route iyw-claw host actions through the live capability catalog.
description: Use when a concrete task needs iyw-claw host state or action, memory learning/maintenance, deep research, or internet/platform evidence and one complete gateway trio is visible. Search the live catalog, read the best match, and invoke its exact current schema. Skip trivial requests and never guess IDs or arguments.
routing:
  capability: iyw-claw host routing
  coreTriggers: [host action, memory, artifact, browser, channel, automation, research, internet]
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
| Memory | current user-memory documents, recall of prior decisions/preferences, or durable memory write |
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
- When the task needs current authoritative user context, search with intent such
  as `read current user memory document`, then read and invoke
  `read_user_memory_documents` for the smallest relevant set of `memory`,
  `profile`, and `soul` documents. Their contents are never injected at launch.
- Skip recall for simple self-contained requests and when the user supplied all
  relevant context. `no_evidence` means no matching evidence, not that the user
  lacks the fact; `unavailable` is a routing limitation.
- When the user explicitly asks to remember a durable fact, preference, or
  correction, search with intent such as `remember confirmed memory`.
- For a conservative reusable fact or correction that may be valuable but was
  not explicitly confirmed, search with intent such as `propose memory`.
- Never store passwords, tokens, cookies, private keys, full credentials,
  transient task state, or speculative claims.
- Do not edit `user-memory.md`, `user-profile.md`, or `user-soul.md` through a
  shell or arbitrary path. Edit them only through the discovered
  `update_user_memory_documents`
  capability with the current overall revision and document eTags. Never use a
  shell path or arbitrary file writer. Host capabilities still own locking,
  transactions, reference integrity, candidate lifecycle, and recall context.

For the complete self-improving behavior mapping, read
[memory-and-learning.md](references/memory-and-learning.md). For a concrete
memory tool's current schema and result handling, read
[tool-usage.md](references/tool-usage.md).

## Deep Research

When the user asks to research, compare, investigate, or gather current web
evidence, follow [research-workflow.md](references/research-workflow.md). Plan
sub-questions, collect multiple sources, deep-read the strongest pages, keep a
claim-to-source ledger, mark uncertainty, and deliver generated reports through
the current Artifacts capability. Use the managed browser and current gateway
catalog; do not copy fixed `/home/clawdbot`, DuckDuckGo, curl, or other external
paths from another Skill.

For platform-specific internet routing and the absorbed Agent Reach behavior,
read [internet-routing.md](references/internet-routing.md) when the request
mentions a platform, URL, social discussion, code search, video, podcast, RSS,
or a login-backed source.

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

## Managed Browser First

Read the installed `agent-browser` Skill for every web page, public web-data,
website automation, or browser verification task. A reliable purpose-built API
or direct data source may run first only when it clearly satisfies the request.
If it returns no data, incomplete data, a static shell for dynamic content, an
authentication boundary, or an unverifiable result, the managed browser is a
mandatory fallback before reporting that the data is unavailable.

Use the current gateway in this order:

1. Search for the exact browser intent and read the best returned capability.
2. List and reuse a managed tab; open another tab only when explicitly needed.
3. Open the target page and use `iyw.browser.page.read.v1` or a fresh snapshot.
4. Use dedicated actions when available. Read `agent-browser` before using
   `iyw.browser.command.run.v1` for an advanced operation.
5. After navigation, a route change, popup, material DOM update, or write, take
   another snapshot before reusing a reference.
6. Verify the business result through URL, title, text, element state,
   downloaded file, or another stable signal. A successful click is not enough.

For a stale reference or locator failure, make one recovery attempt for the
same action with a fresh snapshot and one new reference or revised locator. Do
not cycle locators, switch browsers, or request user takeover for an ordinary
selector problem. For runtime/session/daemon/observer unavailability or a
timeout, inspect managed state once. Only when that check confirms the managed
route is unavailable may the Agent switch to `opencli-browser`, and only when
that Skill and the `opencli` command are actually available. Read its current
Skill and run `opencli doctor`; otherwise report the missing prerequisite.

### Browser User Actions

Use the stable capability `iyw.browser.user_action.request.v1` when the Agent
cannot safely or reliably complete a visible browser step itself, such as
login requiring user-held credentials, MFA, CAPTCHA, device approval, secure
payment confirmation, an interaction the managed browser cannot perform, or a
final human review.
Do not use it for ordinary navigation, snapshots, clicks, fills, waits, or
other actions already covered by the managed browser tools. In particular, do
not request user action merely because a selector was stale, missing, or
ambiguous.

Invoke it through the normal gateway sequence, using the exact schema returned
by `read_iyw_capability`:

1. Search for `browser user action` (or `浏览器 用户 操作`).
2. Read the available `iyw.browser.user_action.request.v1` result.
3. Invoke that exact capability with `reason`, and optionally `tab_id`,
   `completion`, and `timeout_ms`.

The host opens a new visible browser window for the requested tab and pauses
the Agent while the user operates it. Do not ask the user to click a separate
"take over" or "return control" button. The user's first meaningful browser
input is the hand-off signal; the host keeps Agent actions blocked until the
user becomes idle.

Completion conditions are optional. When supplied, all supplied conditions are
required (AND semantics):

```json
{
  "reason": "Complete the sign-in and any verification shown by the website",
  "completion": {
    "urlContains": "/dashboard",
    "textContains": "Sign out"
  },
  "timeout_ms": 180000
}
```

Supported conditions are `urlContains`, `titleContains`, `textContains`,
`selector`, and `downloadCompleted`. Prefer stable post-action evidence such as
an authenticated URL, a success message, a known result element, or a
completed download. Do not put passwords, one-time codes, cookies, tokens, or
other secrets in `reason` or completion conditions. If no condition can be
stated safely, omit `completion`; after the user pauses, inspect the returned
fresh browser state and decide whether the task can continue. A timeout or a
closed detached window is a failed hand-off, not proof that the action
completed.

### Proactive Browser Presentation

When the task produces a web interface, local service page, HTML preview,
visual report, or another browser-readable result that the user should inspect,
proactively use `iyw.browser.window.present.v1` after the page is ready. This
is a non-blocking display action: it opens or focuses a detached browser window
and lets the Agent continue. Do not present every research page or background
automation step; present user-facing results, previews, and meaningful visual
checkpoints.

Use the normal search/read/invoke trio with `browser present` (or `展示网页
成果`) and pass either a fresh `url` (optionally with `new_tab`) or an existing
`tab_id`. Verify that the URL is the intended page before presenting it. A
local service must be reachable through the user's own managed browser; do not
expose credentials or private tokens in the URL.

After the visual result is no longer needed, invoke
`iyw.browser.window.close.v1` with the same `tab_id` (or the active tab). It
closes only the detached display window and preserves the managed tab, page
state, cookies, and sign-in session. Never use this capability when the user
still needs to inspect the page, and never confuse it with
`iyw.browser.tabs.close.v1`, which closes the tab itself.

- Destructive automation, channel, browser, or delegation operations require
  an exact target and the confirmation rules returned by `read`.
- For long work, use a visible feedback capability or discover one at sensible
  checkpoints; use a question capability only for a necessary unresolved input.

## Delivery Receipts

If an invocation returns `iyw_delivery_receipt`, preserve it exactly. On the
next real invocation, send it as the top-level `delivery_ack`, never inside
business arguments. Do not fabricate an invocation just to acknowledge a
receipt.
