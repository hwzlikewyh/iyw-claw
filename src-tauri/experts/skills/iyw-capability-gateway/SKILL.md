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
  `agent-browser`, `wecom-unified`,
   `open-computer-use`, `skill-creator`, `skill-installer`, `plugin-creator`,
   `writing-plans`, or `executing-plans`.
   For any image production or editing request, call the directly advertised
   `generate_iyw_image` tool. It replaces the old `iyw-image-workflows` and
   `imagegen` routing split. Use `type: "auto"` or omit `type` for the shortest
   path; the host chooses ordinary generation, edit, variation, mix, extend,
   fission, or a specialized image operation from the prompt, images, and
   parameters. Do not read another image Skill before this call. Use
   `search_iyw_knowledge` only when the user asks for knowledge-base evidence;
   it is independent and never runs automatically before a normal image task.
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

## Direct Image, Knowledge, and Memory Tools

The HTTP MCP surface exposes three shortest-path tools in addition to the
capability trio:

- `generate_iyw_image`: one-call image generation/editing and all confirmed IYW
  image operations. `type` is optional and defaults to `auto`; put complete
  operation-specific fields under `parameters`. The host waits for terminal
  status and returns public result URLs. HTTPS inputs are submitted directly;
  Data URLs, raw base64, and workspace-local paths are uploaded to TOS without
  `checkImage`.
- `search_iyw_knowledge`: standalone knowledge search with `query`, optional
  `category`, `folderId`, `fileId`, `limit`, and `denseWeight`. It never starts
  an image task.
- `manage_iyw_memory`: memory policy, recall, documents, candidate, harvest,
  settings, append, propose, update, and correction operations grouped under
  one `operation` field. The host performs policy preflight automatically.

### Image shortest paths

Use one call and wait for its result. These examples show the minimum input;
add the complete `parameters` object when the task needs precision.

```json
{"prompt":"白底陶瓷茶壶，现代东方风，产品摄影"}
```

```json
{"type":"variation","prompt":"只把包身改成深绿色防水尼龙，保留版型、拉链、提手和视角","images":["https://example.com/bag.png"]}
```

```json
{"type":"extend","prompt":"保持原图结构和材质语言，延展同系列花瓶","images":[{"url":"https://example.com/vase.png","role":"primary"}],"parameters":{"ratio":"4:3","batchSize":1}}
```

```json
{"prompt":"以第1张产品结构、第2张趋势配色融合成一件可生产餐盘","images":[{"url":"https://example.com/product.png","role":"structure"},{"url":"https://example.com/trend.png","role":"style"}]}
```

```json
{"type":"edit","prompt":"只替换背景为春日窗边自然光，主体大小、边缘和阴影保持不变","images":[{"base64":"...","mimeType":"image/png","role":"source"}],"parameters":{"quality":"high","background":"opaque"}}
```

```json
{"type":"background","prompt":"浅木桌面和自然接触阴影，主体边缘完整","images":["https://example.com/product.png"],"parameters":{"ratio":"1:1","resolution":"standard"}}
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
Non-URL sources are converted to public HTTPS before the image operation. The
decoded input limit is 20 MiB and HTTP image URLs are rejected.

### Knowledge shortest path

```json
{"query":"茶具设计规范","limit":10,"denseWeight":0.5}
```

### Memory shortest path

```json
{"operation":"recall","parameters":{"query":"图片生成默认路径"}}
```

For writes, use the matching operation and pass its exact current fields under
`parameters`; stale candidate revisions/eTags and repair operations without a
preview are rejected by the host.

## Memory Gate

For `manage_iyw_memory`, call the direct tool with the requested `operation`.
It performs the current-turn policy preflight internally, so do not first call
`read_memory_policy`, load another image or memory Skill, or run capability
discovery. The host continues to enforce scope, revision/eTag, candidate
lifecycle, preview, authorization, and error rules. Use the returned
`matched`, `no_evidence`, or `unavailable` state honestly; do not claim that no
history exists from a timeout.

## Do Not Bypass the Host

Never edit host-owned memory documents with shell tools, expose credentials or
provider IDs, use arbitrary browser paths, register internal files as artifacts,
or treat this document as a replacement for the live catalog. The host owns
authorization, locking, idempotency, confirmation, cancellation, persistence,
and result semantics.
