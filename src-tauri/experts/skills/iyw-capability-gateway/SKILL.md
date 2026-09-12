---
name: iyw-capability-gateway
short-description: Route concrete iyw-claw host work through the live capability catalog.
description: >-
  Use proactively when a concrete task needs iyw-claw host state or action:
  memory or self-learning, session/profile/history, final artifacts, managed
  browser or public web evidence, audio transcription, image understanding or
  display, channels and messages, scheduled automation, user interaction, or
  delegated work. First load the matching gateway reference, then use one
  complete search/read/invoke trio when the host catalog is needed for the
  current sub-goal. Prefer a direct tool or domain Skill when it fully owns the
  task. A gateway failure ends that route, not the user's task; hand off to an
  applicable direct tool, domain Skill, or read-only browser workflow without
  guessing IDs, arguments, paths, URLs, or schemas.
routing:
  capability: iyw-claw host routing through live capabilities
  coreTriggers: [host action, memory, self-learning, session, profile, history, artifact, browser, web, internet, audio, transcription, image understanding, channel, message, automation, scheduled task, feedback, question, clarification, ambiguous requirement, needs decision, 需求不清, 需要选择, delegation]
  exclusions: [trivial request, self-contained explanation, direct tool fully covers the task, incomplete gateway trio]
  aliases: [iyw gateway, host capability, capability catalog, 主机能力, 能力网关]
  invocation: Load the matching reference, search the live catalog, read one best match, and invoke its exact current schema when the gateway owns the sub-goal.
---

# IYW Capability Gateway

This Skill is an active routing gate, not a static tool list. The host catalog is
authoritative for current capability IDs, schemas, required inputs, availability,
permissions, and schema digests.

Before first using a tool, read its advertised description and input schema,
including nested fields, required inputs, constraints, and examples. For a
direct tool, this definition is the read; no discovery call is needed. For
capabilities behind `invoke_iyw_capability`, call
`read_iyw_capability` first and read the full result. Common `manage_iyw_memory`
operations (`recall`, `append`, `propose`, `retire`, `documents.read`) include full
inline schemas and need no metadata, Skill or policy read. Maintenance operations
use their advertised capability mapping. IDs are opaque: copy them from
current search/mapping results; never derive them from direct tool names or append versions.

If already read in this conversation, reuse the instructions without another
read, including on later turns and after ordinary parameter errors. Search
summaries alone do not replace the full instructions. Reading before first use
is an Agent rule; the host does not record reads or reject calls based on read
history. The same reuse rule applies to references listed below.

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
  `agent-browser`, `wecom-unified`,
   `open-computer-use`, `skill-creator`, `skill-installer`, `plugin-creator`,
   `writing-plans`, or `executing-plans`.
   For any image production or editing request, call the directly advertised
   `generate_iyw_image` tool. It replaces the old `iyw-image-workflows` and
   `imagegen` routing split. Prefer Fusion `generate` for text-to-image,
   `variation` for single-image changes, `mix` for multi-image fusion, and
   `extend` for four-panel or same-series extension from one reference.
   Explicit Fusion `edit` requires source images. Before `generate`, `auto`
   without images, or `edit`, call `list_iyw_image_models`, choose a model with
   the required capability, and pass its exact ID in `parameters.model`.
   No prior platform attempt or failure is required. Do not read another image
   Skill or search/read a capability ID for generation; none is registered. Use
   `search_iyw_knowledge` only when the user asks for knowledge-base evidence;
   it is independent and never runs automatically before a normal image task.
2. Use this gateway for the remaining iyw-claw host sub-goal: current session
   state, user profile, historical task lookup, memory, artifacts, browser
   host actions, audio, image display/understanding, channels, automation,
   interaction, delegation, or a live plugin capability.
3. Inspect the actual callable surface and select one complete trio of
   `search_iyw_capabilities`, `read_iyw_capability`, and
   `invoke_iyw_capability` when the current host sub-goal requires it. Prefer
   the unique visible `iyw-claw-builtin-*` trio. If the trio is incomplete or
   ambiguous, stop this gateway attempt and use an actually visible direct
   route, owning domain Skill, or read-only browser workflow.

## Mandatory Gateway Sequence

1. Search with 2-5 action/object terms in Chinese or English, such as
   `查询 历史 记忆`, `读取 网页`, `会议 音频 转写`, or `send channel message`.
2. Treat results as current session evidence. Read the best plausible stable
   `capability_id` and its full description/schema; read at most one same-result
   alternative if needed. Reuse a previous full read for the same capability.
3. Invoke only the ID returned by the current search and only with the schema
   returned by the current read. Ask for a missing primary object instead of
   guessing it. Construct arguments from declared fields and examples; omit
   unnecessary optional fields and never borrow parameters from another tool.
4. Verify the business result, status, and any required follow-up. Preserve an
   `iyw_delivery_receipt` exactly as top-level `delivery_ack` on the next real
   invocation.

A `capability_schema_mismatch` with `execution_status=not_started` permits one
correction of the failed operation: use the schema already read and the error's
field hints, preserve the intended behavior, and retry once with corrected
arguments. The same rule applies when only the error text reports schema rejection and
`execution_status=not_started`. Read only if the capability has not been read
before; the error itself does not require another read. Never replay unchanged
arguments or silently drop intended behavior. Stop this recovery if the corrected
call fails.

An empty result, unavailable capability, malformed output, timeout, unknown ID,
schema rejection without that evidence, or two non-matching reads ends the
current gateway attempt. It does not by itself end the user's task. Permission
and effect-unknown errors do not permit this correction. Do not switch
namespaces, invent names, or replay stale arguments. Backend failures do not justify a new ID.

## Route Handoff

Treat capability failures as route-local evidence:

```text
gateway mismatch or failure
  -> stop the current gateway attempt
  -> return to the owning direct tool or domain Skill
  -> for an authenticated web app, use normal browser UI for read-only discovery
  -> reuse only requests actually triggered and verified by that UI
  -> report a blocker only when no safe applicable route remains
```

Normal UI navigation may inspect dynamic menus, fields, and result pages. A
request observed from that UI is reusable only when the page actually triggered
it and the request fields, session context, and business result are verified.
For other failures, follow the owning reference's bounded recovery, then hand
off instead of enumerating cosmetic parameter variations or repeating the same
failed route. Do not chain another recovery after a failed schema correction.
This permits discovery without authorizing guessed endpoints or side effects.

## Direct Interaction Tools

`present_task_files` is also directly advertised for final user-facing files,
directories and public URLs. Register final deliverables in the current
conversation's Artifacts area with one call, then inspect accepted/rejected
results. Do not search/read/invoke first when this direct tool is available.
Select final deliverables from the task without requiring a separate explicit
request for each file. For a PPT containing images, register the completed PPT,
not its embedded images or intermediate materials. Generate intermediate images
with `delivery.registerArtifact: false`; include external companions only when
the final deliverable requires them to work. The same tool supports `list/get`
with `scope=current|all` and current-conversation `update/delete` by artifact ID.
Read [artifact-delivery.md](references/artifact-delivery.md) for delivery scope.

Use the directly advertised `ask_user_question` for a concrete user-owned input,
preference or decision: clarify requirements and scope, supply missing information,
choose an approach, or give concise feedback. Options and header are optional;
free text is always available. Ask only necessary questions and wait for the answer.
Do not use it for routine progress confirmation or information you can find yourself.

Proactively use `show_interactive_html` when seeing or manipulating something helps
the user understand, explore, compare, express or decide. Freely design the HTML,
CSS, JavaScript, SVG, Canvas, layout, visual style, interactions and returned JSON.
Interactive explanations, simulations, design previews, annotations, configurable
charts and custom mini-tools are examples, not restrictions. The page loads
automatically; no local server or preview click is needed. Use a complete document
with inline code and embedded assets. Default presentation returns immediately;
set `wait_for_response: true` when the next step needs the user's result, and call
`await iyw.submit(data)` from an explicit page action. Handle errors and preserve
the user's draft. Use `ask_user_question` when a short question or options suffice.

Call both tools directly when advertised; do not search/read/invoke first. See
[interaction-tools.md](references/interaction-tools.md) for the page bridge and
examples. If a direct tool is unavailable, use ordinary conversation or discover
the existing question capability through the live catalog; never invent an alias.

## Direct Image, Knowledge, and Memory Tools

The HTTP MCP surface also exposes these shortest-path tools alongside interaction
tools and the capability trio:

- `list_iyw_image_models`: read-only Fusion image model catalog; call with `{}`.
  Returns IDs, names, descriptions, generation/editing capabilities, and prices.
  The agent selects the model for `generate`/`edit` and passes its exact ID in
  `parameters.model`. Specialized IYW operations do not use this catalog.
- `generate_iyw_image`: image generation/editing and all confirmed IYW
  image operations. Prefer an explicit `type`; put supported operation-specific
  fields under `parameters`. The host waits up to the requested timeout and
  returns status, task IDs when available, and public result URLs. Specialized
  tools reuse HTTPS inputs and upload local/base64 inputs to TOS without
  `checkImage`. `edit` submits local/base64 bytes directly to Fusion.
- `search_iyw_knowledge`: standalone knowledge search with `query`, optional
  `category`, `folderId`, `fileId`, `limit`, and `denseWeight`. It never starts
  an image task.
- `manage_iyw_memory`: memory policy, recall, documents, candidate, harvest,
  settings, append, propose, update, and correction operations grouped under
  one `operation` field. Read each selected operation's capability instructions
  first; the host performs policy execution preflight automatically.

### Image shortest paths

Choose the route from the task:

| Task | Preferred type |
| --- | --- |
| Text-to-image, without source images | `generate` through Fusion `images/generations` |
| Redesign or modify one source image | `variation` |
| Fuse 2-10 reference images | `mix`, preserving input order |
| Four-panel grids or same-series extension from one base image | `extend` |

Fusion `edit` (`images/edits`) remains available when explicitly selected and
requires at least one source image. `generate` and `edit` do not require a prior
platform attempt or failure. `fission` remains an explicitly selectable platform
operation. Select the matching specialized operation for background, outpaint,
super-resolution and similar tasks.

Before `generate`, `auto` without images, or explicit `edit`, call
`list_iyw_image_models` with `{}`. Select from the returned descriptions,
capabilities, prices and task requirements: `generate` needs
`capabilities.image_generation=true`; `edit` needs
`capabilities.image_editing=true`. Pass the exact returned `id` in
`parameters.model`. Missing IDs and display names are rejected; the host does
not choose a default model. Reuse the catalog for the same task or batch;
an empty or failed lookup supplies no model. Platform operations need no Fusion
model lookup. Replace `MODEL_ID_FROM_CATALOG` in examples before calling.

`auto` uses `generate` without images; with one image it uses `extend` for
series, extension, four-panel or 2x2 wording, and `variation` otherwise; with
multiple images it uses `mix`. Prefer an explicit type when the intent is known.
An `extend` request needs one base image: describe the four-panel layout or series
in its prompt. Without a reference, create the requested composition with
`generate`. Keep multiple references for fusion; for independent series derived
from multiple bases, use separate `requests` items, each with one base image.
Do not discard references to force the single-image extension interface.

Inspect the returned operation when explaining routing. After a timeout,
transport error or uncertain submission, query the original task ID when available;
do not blindly resubmit or switch routes. Deliver ordinary successful images from
returned status, URLs and delivery metadata. Inspect visuals for requested review,
comparison, visual acceptance or integration into a composed deliverable.
For intermediate images used in a PPT/report/webpage, set
`delivery.registerArtifact: false` and register only the final deliverable.

**Timeouts:** omit `wait.timeoutSeconds` for 600 seconds on platform requests and
polling, or 300 seconds on `generate`/`edit`. The agent can explicitly override
either, including values above 600; prefer the defaults or longer for slow tasks.
Do not shorten waits merely to return sooner. Each batch item has its own wait.
`0` means submit without polling on the platform; Fusion keeps its default timeout.

```json
{"type":"generate","prompt":"白底陶瓷茶壶，现代东方风，产品摄影","parameters":{"model":"MODEL_ID_FROM_CATALOG"},"wait":{"timeoutSeconds":900}}
```

```json
{"type":"variation","prompt":"只把包身改成深绿色防水尼龙，保留版型、拉链、提手和视角","images":["https://example.com/bag.png"]}
```

```json
{"type":"extend","prompt":"保持原图结构和材质语言，设计四宫格同系列花瓶，每格一种配色","images":[{"url":"https://example.com/vase.png","role":"primary"}],"parameters":{"ratio":"4:3","batchSize":1}}
```

```json
{"type":"mix","prompt":"以第1张产品结构、第2张趋势配色融合成一件可生产餐盘","images":[{"url":"https://example.com/product.png","role":"structure"},{"url":"https://example.com/trend.png","role":"style"}]}
```

When Fusion editing is explicitly selected:

```json
{"type":"edit","prompt":"以原图为参考自由重绘为超现实拼贴海报，重新设计透视和构图，保留主体标识，右侧留出标题区域","images":[{"base64":"...","mimeType":"image/png","role":"source"}],"parameters":{"model":"MODEL_ID_FROM_CATALOG"}}
```

```json
{"type":"background","prompt":"浅木桌面和自然接触阴影，主体边缘完整","images":["https://example.com/product.png"],"parameters":{"size":"1:1","resolution":"standard"}}
```

```json
{"type":"super-resolution","images":["https://example.com/low-res.png"],"parameters":{"upscale":4}}
```

```json
{"type":"line-extraction","images":["https://example.com/product.png"],"parameters":{"model":"canny","batch_size":1,"stats":{"reference":"https://example.com/product.png"}}}
```

```json
{"type":"image-to-3d","images":["https://example.com/product.png"],"parameters":{"stats":{"format":1,"MultiViewImages":[]}}}
```

```json
{"type":"video","prompt":"镜头从正面缓慢环绕，展示材质高光","images":["https://example.com/product.png"],"parameters":{"ratio":"16:9","duration":8,"mode":"normal"}}
```

商品套图、AI 试衣、出血线和色号提取只可在其页面当前服务给出完整、已确认的
请求契约后接入；网关不会按页面名称猜 endpoint 或 payload。其他专用操作必须显式
传 `type`，并将全部细节传到 `parameters`。

For a local path use `"images":["assets/product.png"]`; for raw base64 use an
object with `base64` and `mimeType`; for a Data URL pass it as the string source.
For IYW image tools, non-URL sources become public HTTPS before execution;
`edit` uses their validated bytes directly. The decoded input limit is 20 MiB
and HTTP image URLs are rejected.

### Knowledge shortest path

```json
{"query":"茶具设计规范","limit":10,"denseWeight":0.5}
```

### Memory shortest path

Read the direct tool's advertised schema and call it immediately:

```json
{"operation":"recall","parameters":{"query":"图片生成默认路径"}}
```

For `append`, `propose` and `retire`, use the inline schema directly. Pass fields under
`parameters`; stale candidate revisions/eTags and repair operations without a
preview are rejected by the host.

## Memory Gate

For `manage_iyw_memory`, use inline schemas for recall, append, propose, retire
and documents.read. Other operations need one read of their mapped capability.
Put business fields under `parameters`. No catalog search is needed for advertised
mappings. The tool performs current-turn policy execution internally.
The host continues to enforce scope, revision/eTag, candidate lifecycle, preview,
authorization, and error rules. Use the returned
`matched`, `no_evidence`, or `unavailable` state honestly; do not claim that no
history exists from a timeout.

## Do Not Bypass the Host

Never edit host-owned memory documents with shell tools, expose credentials or
provider IDs, use arbitrary browser paths, register internal files as artifacts,
or treat this document as a replacement for the live catalog. The host owns
authorization, locking, idempotency, confirmation, cancellation, persistence,
and result semantics.
