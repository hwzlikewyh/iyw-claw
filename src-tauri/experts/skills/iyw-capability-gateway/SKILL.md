---
name: iyw-capability-gateway
short-description: 爱原物业务接口、图片、上传与 iyw-claw 主机能力的分层调用指南。
description: >-
  Use for 爱原物/IYW 设计云、AI工作台、图案网的业务接口：产品与标签、客户需求、
  趋势报告、知识库目录、原助理会话、IP/图案/授权、瓶型瓶盖、Temu、会员点数钱包、
  组织员工、需求比稿、版权合同、店铺展厅、素材收藏、任务进度和PDF；包含商品套图/A+、
  批量中心、知识库切片/附件、资产库、工厂订单/物流、权益分层；也用于图片处理、
  任意文件上传(50MiB)、fetch_iyw_url。按关键词索引逐层读取参数，剩余业务统一用
  fetch_iyw_url，图片用generate_iyw_image，上传用upload_iyw_file。
  Also route iyw-claw memory/learning, session/profile/history, artifacts,
  browser/web evidence, audio, image understanding, channels/messages,
  automation, interaction and delegation through the matching reference and
  live host catalog. Read only relevant references; never guess IDs or schemas.
routing:
  capability: IYW business APIs through fetch_iyw_url and iyw-claw host capabilities
  coreTriggers: [host action, memory, self-learning, session, profile, history, artifact, browser, web, internet, audio, transcription, image understanding, channel, message, automation, scheduled task, feedback, question, clarification, ambiguous requirement, needs decision, 需求不清, 需要选择, delegation, 爱原物, 设计云, 产品, 标签, 客户需求, 趋势报告, 图案, IP授权, 版权, 点数, 钱包, 组织, 店铺, 瓶型, Temu, 上传文件, fetch_iyw_url, upload_iyw_file]
  exclusions: [trivial request, self-contained explanation]
  aliases: [iyw gateway, host capability, capability catalog, 主机能力, 能力网关, 爱原物接口, 设计云, AI工作台, 图案网, 产品库, 版权登记, 文件上传]
  invocation: For IYW business tasks load iyw-api-index and the matching domain reference, then call fetch_iyw_url. Images and uploads use their direct tools. Search/read/invoke only for host catalog capabilities.
---

# IYW Capability Gateway

This Skill is an active routing gate, not a static tool list. The host catalog is
authoritative for current capability IDs, schemas, required inputs, availability,
permissions, and schema digests.

爱原物业务先读 [接口索引](references/iyw-api-index.md)，按关键词仅加载对应领域。
除图片生成/处理与通用上传外，本资料的业务接口全部通过 `fetch_iyw_url` 执行。
业务 API 路由不依赖能力三件套；主机能力才使用下文 catalog 流程。

Before first using a tool, read its advertised description and input schema,
including nested fields, required inputs, constraints, and examples. For a
direct tool, this definition is the read; no discovery call is needed. For
capabilities behind `invoke_iyw_capability`, call
`read_iyw_capability` first and read the full result. Each `manage_iyw_memory`
operation uses its advertised capability mapping. IDs are opaque: copy them from
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
| 商品套图/A+、批量图片、新版 Agent、知识库全套、资产库、订单物流、权限分层 | [接口索引新版任务表](references/iyw-api-index.md)，仅加载对应补充资料 |
| 爱原物产品/标签、客户需求、趋势/IP/图案、会员点数、组织、版权、设计云或具体 API | [业务接口索引](references/iyw-api-index.md)，再读匹配领域与 [HTTP 约定](references/iyw-http.md) |
| 上传任意文件、压缩包、文档、音视频、50M 文件链接 | [通用上传](references/iyw-upload.md) |
| 图片生成/处理、扩图、放大、抠图、消除、色号、矢量、3D、视频 | [图片工具参数](references/iyw-image-tools.md) |
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
- `fetch_iyw_url`: 所有已记录的剩余爱原物业务接口入口；传入 description、url、method、query/body。
  先查 [接口索引](references/iyw-api-index.md)，不搜索 capability_id。
- `upload_iyw_file`: 任意类型工作区文件，最多 50 MiB；description + path，可选 name/mime_type。
  取得公开 URL 后按用户任务用 fetch 保存业务记录或交付文件。
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

### 图片参数按需读取

常用默认路径：无图文生图用 `generate`，单图改款用 `variation`，有基准图的四宫格或同系列延伸用 `extend`，多图融合用 `mix`。
`generate` 和显式 `edit` 无需先等平台失败，但需从模型目录选择准确 ID；`edit` 仍要求参考图。
专用处理、批量、蒙版、色号、矢量、3D、视频和模型选择见
[图片工具参数](references/iyw-image-tools.md)；只读本次操作相关部分。
原始图片 API、旧/新参数差异和历史点数见
[图片接口证据](references/iyw-image-api-source.md)，不默认加载。

### Knowledge shortest path

```json
{"query":"茶具设计规范","limit":10,"denseWeight":0.5}
```

### Memory shortest path

First call `read_iyw_capability` with the ID advertised for `recall`,
`iyw.memory.recall.search.v1`, and read its full description and input schema.
Then call the direct memory tool with:

```json
{"operation":"recall","parameters":{"query":"图片生成默认路径"}}
```

For writes, use the matching operation and pass its exact current fields under
`parameters`; stale candidate revisions/eTags and repair operations without a
preview are rejected by the host.

## Memory Gate

For `manage_iyw_memory`, read the requested operation's instructions using its
advertised capability ID, then call the direct tool with `operation` and its
schema fields under `parameters`. No catalog search is needed for the advertised
mapping. The tool performs current-turn policy execution internally; reading the
operation instructions does not replace or require manually invoking that policy.
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
