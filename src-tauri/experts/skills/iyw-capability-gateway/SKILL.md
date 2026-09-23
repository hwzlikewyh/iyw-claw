---
name: iyw-capability-gateway
short-description: 爱原物业务接口、图片、视频、商品套图、上传与主机能力的分层调用指南。
description: >-
  Use for 爱原物/IYW 设计云、AI工作台、图案网的业务接口：产品与标签、客户需求、
  趋势报告、知识库目录、原助理会话、IP/图案/授权、瓶型瓶盖、Temu、会员点数钱包、
  组织员工、需求比稿、版权合同、店铺展厅、素材收藏、任务进度和PDF；包含商品套图/A+、
  电商视频/产品演绎/视频复刻、爆款复刻、批量中心、知识库切片/附件、资产库、工厂订单/物流、权益分层；也用于图片处理、
  任意文件上传(1GiB)、fetch_iyw_url。按关键词索引逐层读取参数，剩余业务统一用
  fetch_iyw_url，图片用generate_iyw_image，上传用upload_iyw_file。视频生成优先走本 Skill
  的 generate_iyw_image/fetch_iyw_url，按视频专篇匹配当前工具与页面契约。
  Also discover signed-in enterprise/profile/prospecting/email, customs/trade,
  ecommerce/brand review and other remote business tools when direct routes do not cover the task.
  Also route iyw-claw memory/learning, session/profile/history, artifacts,
  browser/web evidence, audio, image understanding, channels/messages,
  automation, interaction and delegation through the matching reference and
  live host catalog. Read only relevant references; never guess IDs or schemas.
routing:
  capability: IYW business APIs through fetch_iyw_url and iyw-claw host capabilities
  coreTriggers: [host action, memory, self-learning, session, profile, history, artifact, browser, web, internet, audio, transcription, image understanding, channel, message, automation, scheduled task, feedback, question, clarification, ambiguous requirement, needs decision, 需求不清, 需要选择, delegation, 爱原物, 设计云, 产品, 标签, 客户需求, 趋势报告, 图案, IP授权, 版权, 点数, 钱包, 组织, 店铺, 瓶型, Temu, 视频生成, 电商视频, 产品演绎, 视频复刻, 自动导演, 商品套图, A+, Listing, 爆款复刻, 上传文件, fetch_iyw_url, upload_iyw_file]
  exclusions: [trivial request, self-contained explanation]
  aliases: [iyw gateway, host capability, capability catalog, 主机能力, 能力网关, 爱原物接口, 设计云, AI工作台, 图案网, 产品库, 版权登记, 文件上传]
  invocation: For IYW business tasks load iyw-api-index and the matching domain reference. Prioritize video generation through generate_iyw_image or fetch_iyw_url as documented in iyw-api-ecommerce-video. Images and uploads prefer their direct tools; documented workflow mismatches use fetch_iyw_url. Search/read/invoke covers remaining host and remote business capabilities.
---

# IYW Capability Gateway

记忆相关能力统一使用 `manage_iyw_memory`，不为每个动作寻找一个独立 MCP 工具。
需要维护记忆、处理纠正或排查失效时，读 [记忆策略](references/memory-and-learning.md)
和 [调用示例](references/memory-examples.md)。示例覆盖保存、范围例外、旧信息停用、
候选及后台复核、冲突重读和失败恢复；常规维护由 Agent 在任务中完成。

This Skill is an active routing gate, not a static tool list. The host catalog is
authoritative for current capability IDs, schemas, required inputs, availability,
permissions, and schema digests.

The same search/read/invoke trio includes signed-in remote business capabilities.
Use search `source=local` for host actions, `source=remote` for enterprise/profile/
prospecting/email, customs/trade, ecommerce products, brand reviews, copyright,
product tags/patterns, web search, speech or retrieval; omit source when unsure.
Existing direct tools and documented business API routes keep their priority.
Use remote discovery when those routes do not cover the requested subgoal,
before claiming it unsupported.

Read a group's workflow and relevant `items`: members have their own
`capability_id`, complete `input_schema` and `usage` (`use_when`, `argument_sources`,
`result_summary`). A fully read member needs no additional read. Invoke its ID,
never the group; the host carries remote versions. Remote instruction examples
are usage guidance for this same trio, not permission to invent callable names
or put routing fields in arguments. `remote_catalog.status=unavailable` is not
evidence of absence; local matches remain usable. `TOOL_CHANGED` with
`execution_status=not_started` requires rereading the old capability_id and using
the current member ID returned. `remote_catalog_expired` permits one fresh search.
Unknown execution outcomes require original task/status evidence before replay.
These recovery cases take precedence over the generic stop rules below.

爱原物业务先读 [接口索引](references/iyw-api-index.md)，按关键词仅加载对应领域。
图片生成/处理优先用 `generate_iyw_image`，通用上传用 `upload_iyw_file`；其余业务通过 `fetch_iyw_url` 执行。
电商视频和商品套图中已记录的工具契约不匹配或未封装操作，按对应参考用 fetch 调原接口。
已记录的业务 API 路由不依赖能力三件套；主机能力及未覆盖的远程业务使用下文 catalog 流程。

## 最短适用路径

- 自定义改款优先 `variation`，单基准图系列延伸优先 `extend`，多图融合优先 `mix`，电商视频优先下文平台路线。按真实输入、目标和限制匹配；其他工具按实际功能选择。
- 仅在用户明确选择其他服务，或优先路线有明确不支持、不可用、确认失败的证据时改用其他路线；已知不兼容无需先试错。上述平台图片工具无需查模型目录。
- 内置转 3D 已禁用，不调用 `image-to-3d`，不通过 fetch、浏览器或其他封装绕过；不得把具有立体效果的图片当作 3D 模型交付。
- 已公布且参数明确的工具直接调用；只补读缺失的当前操作说明。复用已读 schema、参考资料、模型目录、素材 URL 和任务 ID，支持的独立操作按授权范围批量执行。
- 仅使用当前启用目录公布的技能。废弃、停用、移除或仅残留在历史会话/备份中的技能不读取、不执行、不恢复；遇到旧依赖立即选择现有工具，不沿旧依赖链搜索或安装。`iyw-image-workflows` 已废弃。
- 本地文件、代码、文档用本地工具；平台业务用专用工具；公开资料优先搜索/正文读取。仅登录态、动态内容无法直接读取、页面交互、截图或页面验收需要浏览器；确定使用后再读浏览器参考和查标签页。

## 视频生成优先路由

视频生成任务优先走本 Skill 的爱原物能力，先读 [电商视频](references/iyw-api-ecommerce-video.md)。
单图、4-15 秒且符合现有参数时用 `generate_iyw_image(type=video)`；电商产品演绎、
多图、1-3 秒或视频复刻等页面契约用 `fetch_iyw_url` 调 `videoGenerator`。
自动导演/复刻导演只返回脚本，任务查询与历史管理用 fetch；返回任务 ID 不代表已完成视频。
先匹配这两条现有路径；用户明确指定其他服务，或适用路径有明确不支持/不可用/失败证据时，才考虑其他路线。
超时或提交结果未知先查原任务，不通过换工具重复生成；不得编造视频 capability_id。

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
| 视频生成、电商视频、产品演绎、视频复刻、自动导演、视频历史 | [电商视频](references/iyw-api-ecommerce-video.md)，按实际工具契约选择 generate 或 fetch |
| 商品套图/A+、Listing、商品卖点、爆款复刻、图片版本 | [商品套图](references/iyw-api-product-kits.md) |
| 批量图片、新版 Agent、知识库全套、资产库、订单物流、权限分层 | [接口索引新版任务表](references/iyw-api-index.md)，仅加载对应补充资料 |
| 爱原物产品/标签、客户需求、趋势/IP/图案、会员点数、组织、版权、设计云或具体 API | [业务接口索引](references/iyw-api-index.md)，再读匹配领域与 [HTTP 约定](references/iyw-http.md) |
| 上传任意文件、压缩包、文档、音视频、1G 文件链接 | [通用上传](references/iyw-upload.md) |
| 图片生成/处理、扩图、放大、抠图、消除、色号、矢量 | [图片工具参数](references/iyw-image-tools.md)，只补读当前操作缺失的参数 |
| Session, profile, history, interaction, or plugin capability | [capability-families.md](references/capability-families.md) |
| Unclear requirement, missing decision, or multiple reasonable interpretations | [capability-families.md](references/capability-families.md) |
| Final file, directory, URL, HTML/Markdown delivery, or image references in a document | [artifact-delivery.md](references/artifact-delivery.md) |
| Channel discovery, targets, messages, credentials, QR authorization, or connection state | [channel-operations.md](references/channel-operations.md) |
| Scheduled task project selection, cron, create, update, pause, or delete | [automation.md](references/automation.md) |
| Independent subtask, parallel Agent, task ID, wait, or cancellation | [delegation.md](references/delegation.md) |
| Required browser interaction, screenshot, page acceptance, audio, transcription, or image understanding | [browser-and-media.md](references/browser-and-media.md)，只读对应部分 |
| Prior decisions, preferences, repeated workflows, memory, learning, correction, candidate, or memory repair | [memory-and-learning.md](references/memory-and-learning.md) |
| Skill usage failure, recurring workaround, verified improvement or Skill evolution | [skill-evolution.md](references/skill-evolution.md) |
| Research, comparison, investigation, current web evidence, or cited report | [research-workflow.md](references/research-workflow.md), plus [browser-and-media.md](references/browser-and-media.md) for browser work |
| Research/read an existing platform, URL, social discussion, GitHub, video, podcast, RSS, finance, or login-backed source (not video generation) | [internet-routing.md](references/internet-routing.md) |
| Unsure which family or how to call the trio | [tool-usage.md](references/tool-usage.md) |

## Route Proactively

1. Use an exact visible direct tool when it fully satisfies the current
   sub-goal. Otherwise use a currently enabled domain Skill that owns the workflow:
  `agent-browser`, `wecom-unified`,
   `open-computer-use`, `skill-creator`, `skill-installer`, `plugin-creator`,
   `writing-plans`, or `executing-plans`.
   For video generation, apply the video priority and contract routing above.
   For image generation, editing, and processing, prioritize platform capabilities
   or image models through the directly advertised `generate_iyw_image` tool.
   Before installing image libraries or writing processing code, match the user's
   effect and inputs to a service. A missing specialized operation or sparse
   description does not prove unsupported editing: evaluate prompt-driven editing
   or the image model catalog. Verify technical constraints against documentation;
   broad flags do not guarantee masks, transparency, bit depth, or dimensions.
   Do not invent requirements or discard real ones. Prefer Fusion `generate`
   for text-to-image, Fusion `edit` for source images with a selected model,
   `variation` for single-image changes, `mix` for multi-image fusion, and
   `extend` for four-panel or same-series extension from one reference.
   Explicit Fusion `edit` requires source images. Before `generate`, `auto`
   without images, or `edit`, call `list_iyw_image_models`, choose a model with
   the required capability, and copy its opaque `model_ref` into `parameters.model`.
   Prefer applicable `variation`, `extend`, and `mix` routes. Use model editing
   when explicitly selected or the preferred route is demonstrably unsupported,
   unavailable, or confirmed failed; do not manufacture a failed trial.
   A catalog model argument on a platform
   type does not select that Fusion model. Correct input errors; evaluate another
   suitable service after confirmed route failure. Timeouts, transport errors,
   and running/uncertain tasks require state checks before new submissions.
   For inadequate visual results, prefer a targeted service edit within scope and
   charging authorization. Before manual fallback, require concrete limitations
   of both applicable platform and model routes after bounded recovery; explain
   them without enumerating unrelated services. A failed auxiliary step alone
   does not qualify. Copying/downloading inputs and embedding completed images
   are allowed; redrawing, segmentation and height remapping remain core image
   work even when called refinement or post-processing. In user-facing text use
   only business display names, never model/provider IDs or backend names, and
   claim only the operation/model confirmed by results. Do not read another
   image Skill or search/read a capability ID for generation; none is registered. Use
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

For image work, the platform/model priority and fallback conditions above still
apply; a gateway failure alone does not authorize manual image processing.

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
  Returns opaque `model_ref` values, business display names, safe descriptions, and generation/editing capabilities; no real IDs, providers, or prices.
  The agent selects the model for `generate`/`edit` and copies its `model_ref` into
  `parameters.model`. Specialized IYW operations do not use this catalog.
- `fetch_iyw_url`: 所有已记录的剩余爱原物业务接口入口；传入 description、url、method、query/body。
  包括视频/套图参考明确指定的页面契约；先查 [接口索引](references/iyw-api-index.md)，不搜索 capability_id。
- `upload_iyw_file`: 任意类型工作区文件，最多 1 GiB；description + path，可选 name/mime_type。
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
缺少同名专用操作时，继续评估通用指令编辑或模型，不能据此判断不支持；格式、尺寸等技术限制按实际文档核对。按下方图片参考中的失败分类恢复，核心图片精修同样遵守服务优先规则。
`generate` 用于无图创作；`edit` 遵循上文平台优先及替代条件，有参考图且选择模型时使用。两者需将模型目录返回的 `model_ref` 原样放入 `parameters.model`；同任务复用目录。引用仅用于内部参数，对用户只说业务展示名称或“通用图片处理”；即使追问模型、价格或锁定选型，也不披露真实模型 ID、引用、供应商和货币价格。仅可说明平台明确返回的点数，不换算、不猜测。
视频优先规则与完整流程见 [电商视频](references/iyw-api-ecommerce-video.md)。专用处理、批量、蒙版、色号、矢量和模型选择见
[图片工具参数](references/iyw-image-tools.md)；只读本次操作相关部分。
原始图片 API、旧/新参数差异和历史点数见
[图片接口证据](references/iyw-image-api-source.md)，不默认加载。

### Knowledge shortest path

```json
{"query":"茶具设计规范","limit":10,"denseWeight":0.5}
```

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
