# 新版 Agent 会话、轮次与 SSE

来源：2026-09-10《爱原物接口文档·分层详版（补充篇）》；前端反解和原采集者实测标记，不表示本次已调用。只读取当前任务相关小节；其他领域见 [接口索引](iyw-api-index.md)，冲突和认证/签名边界见 [调用约定补充](iyw-api-access-contracts.md)。

关键词：新版 Agent、本体智能体、电商智能体、会话偏好、深度思考、候选回答、run_id、SSE、断线恢复。

这里所有业务操作均走 fetch_iyw_url，包含文档明确的 DELETE；保留线上 /ai-agent-new/api/agent 前缀，禁止猜开发域或提供开发 tokenInfo。涉及 Path 的 session_id/turn_id 必须分别 URL 编码，agent_id/session_id 查询字段仍放 query。

聊天 timeout_seconds 可设 300；fetch 会等到 SSE 结束再返回文本，不实时转发事件。完整响应按 event/type 识别 session_info/run_info/error/done；超时、取消、断流都不代表未发消息。恢复先 GET runs/current + messages，再决定是否继续等或按用户要求停止；不要重复 chat/regenerate。getAndDelete 会消耗草稿，不能当普通可重复查询。

```json
{"description":"查询电商智能体会话","url":"https://gateway.iyw.cn/ai-agent-new/api/agent/sessions","method":"GET","query":{"agent_id":1100001,"page":1,"page_size":30}}
```

## 来源详细资料

## 四、新版 Agent 会话全套（`ai-agent-new/api/agent`，之前文档只有一行「会话全套」）

这一组是 `ai.iyw.cn`「本体智能体 / 电商智能体」页面（路由 `/agent_new`）真正在跑的接口。源码位置：`ai_7323.d6740932.js`（主页面 + 知识库）、`ai_4159.11b16342.js`、`ai_9084.61939702.js`（模块化封装）、`ai_7221.f10fe826.js`（商品套图页复用同一套会话接口）。

### 4.0 前缀、基址与请求头

| 项 | 值 | 说明 |
| --- | --- | --- |
| 基址 | `https://gateway.iyw.cn` | 固定写死 |
| 前缀（线上） | `/ai-agent-new/api/agent` | `VUE_APP_AGENT_NEW_API_PREFIX` 未配置时的默认值 |
| 前缀（开发） | `/api/agent` | Cookie 存在 `agent_new_dev_token` 时切换，同时基址可用 `VUE_APP_AGENT_NEW_API_ORIGIN` 覆盖 |
| 请求头 | `Content-Type: application/json` | 全部接口 |
| 请求头 | `token` | 取 Cookie `iyuanwu_token`（开发模式优先 `agent_new_dev_token`）；非开发模式由 axios 拦截器自动注入 |
| 请求头 | `tokenInfo` | 仅开发模式带，取 Cookie `agent_new_dev_tokenInfo` / `tokenInfo` / `agent_chat_token_info` |
| 超时 | 默认 300 秒 | 与全局 axios 一致 |

**重要**：`/sessions`、`/messages`、`/runs/*`、`/favorites` 这些接口里带的 `agent_id` 不是可选项，它决定你读的是哪个智能体的会话空间。取值表：

| `agent_id` | 智能体 | 页面路由 | 说明 |
| --- | --- | --- | --- |
| `1` | 本体智能体 | `/agent_new` | 默认值，能分析图片/PDF、生成方案、匹配产品、改图生图 |
| `2` | 需求理解与设计 | — | 需权益 `I12`，无权益时前端直接弹工单 `C-ZXXQLJYSJ` |
| `100` | AI 设计（企微版） | — | 出现在 `ai_4159` 的常量表 |
| `100100` | AI 设计子通道 | — | 同上 |
| `1100001` | 电商智能体 | `/agent_new?agent_id=1100001` | 走商品套图；提示词可带「产品名称/电商平台/图片比例/输出语言/目标国家」 |

### 4.1 会话管理（7 个接口）

| 请求类型 | 接口 | 功能 | 入参 | 参数含义 | 返回 |
| --- | --- | --- | --- | --- | --- |
| GET | `/sessions` | 会话列表（分页） | Query：`page`（默认 1）、`page_size`（默认 50，页面实际用 30）、`agent_id`、`keyword`（可选，会话搜索） | `agent_id` 见上表；`keyword` 会进入会话搜索弹窗，按会话标题模糊匹配 | `data` 为数组，每项含 `session_id`、`session_title`、`created_at`、`updated_at`、`is_favorite`、`thinking_enabled`、`raw`（原始对象） |
| POST | `/favorites` | 收藏会话 | Body：`agent_id`、`session_id` | 把某个会话加入收藏，前端随后刷新列表 | `code=1` 即可，`data` 一般为空 |
| DELETE | `/favorites/{session_id}` | 取消收藏 | Path：`session_id`（URL 编码）；Query：`agent_id` | `session_id` 走 `encodeURIComponent` 拼接 | 同上 |
| DELETE | `/sessions/{session_id}` | 删除会话 | Path：`session_id`；Query：`agent_id` | 删除后前端把该行从列表移除；若正在生成会先提示「生成中，请稍后再删」 | 同上 |
| POST | `/sessions/preferences` | 修改会话偏好 | Body：`agent_id`、`session_id`、`thinking_enabled` | `thinking_enabled` 为布尔，控制该会话是否开启「深度思考」；前端同时更新本地会话行的 `thinking_enabled`（1/0） | 同上 |
| GET | `/messages` | 拉取某会话全部消息轮次 | Query：`session_id`、`agent_id` | 返回的是「轮次（turn）」数组，不是平铺消息 | 每项含 `turn_id`、`response_content`、`reasoning_content`、`tool_calls[]`、`variant_id`、`selected_variant_id`、`variant_no`、`variant_count`、`variants[]`、`is_last_turn`，以及用户侧内容字段（见 4.3） |
| GET | `/runs/current` | 查询该会话当前是否有正在执行的任务 | Query：`session_id`、`agent_id` | 页面刷新/断线重连时用它恢复「正在生成」状态 | `data.run` 含 `run_id`、`run_kind`（`new_turn` / `regenerate`）、`request_user_message_content`、`request_user_message_format` |

### 4.2 消息与轮次操作（4 个接口 + 2 个流式接口）

| 请求类型 | 接口 | 功能 | 入参 | 参数含义 |
| --- | --- | --- | --- | --- |
| POST | `/runs/stop` | 停止当前生成 | Body：`agent_id`、`session_id`、`run_id` | `run_id` 来自 `/runs/current` 或 SSE 的 `run_info` 事件；不传 `run_id` 停不掉 |
| DELETE | `/messages/{turn_id}` | 删除最后一轮消息 | Path：`turn_id`；Query：`session_id`、`agent_id` | 前端只在「最后一条」上给删除入口 |
| POST | `/messages/edit` | 修改某一轮的回答内容 | Body：`agent_id`、`session_id`、`turn_id`、`response_content` | `response_content` 是用户在弹窗里改写后的整段回答，保存后重新拉 `/messages` 并刷新会话列表 |
| POST | `/messages/select-variant` | 在多个候选回答间切换 | Body：`agent_id`、`session_id`、`turn_id`、`variant_id` | `variant_id` 来自该轮次 `variants[]`；只切换选中项，不重新生成 |
| POST | `/chat` | **发消息（SSE 流式）** | Body 见下 | 用原生 `fetch` 而非 axios，因为要读 `ReadableStream` |
| POST | `/regenerate` | **重新生成（SSE 流式）** | Body：`agent_id`、`session_id`、`turn_id`、`extra_input` | `extra_input` 结构与 `/chat` 相同 |

#### `/chat` 请求体逐字段说明（源码原文反解）

```json
{
  "agent_id": 1,
  "session_id": "或 null，首次对话为 null",
  "text": "用户输入的文本，纯图片时可传 null",
  "files": [
    { "type": "image_url", "value": "https://.../x.png" },
    { "type": "file_url", "value": "https://.../x.pdf" }
  ],
  "extra_input": {
    "use_thinking": false,
    "session_config_patch": {
      "model_options": [ { "platform": 2 } ]
    },
    "knowledge_mount": {
      "0": { "folder_ids": ["12"], "file_ids": ["88"] },
      "1": { "folder_ids": [], "file_ids": ["99"] }
    }
  }
}
```

| 字段 | 类型 | 必填 | 作用 |
| --- | --- | --- | --- |
| `agent_id` | number | 是 | 目标智能体，见 4.0 表 |
| `session_id` | string \| null | 否 | 首次对话传 `null`，服务端会在 SSE 的 `session_info` 事件里回传新 id |
| `text` | string \| null | 否 | 用户文本；纯上传文件时可为 `null`，但不能和 `files` 同时为空 |
| `files[].type` | string | 否 | 仅两种：`image_url`（图片）、`file_url`（文档）；前端会把 `file_id` 也归一到 `file_url` |
| `files[].value` | string | 否 | 上传后的可访问地址（图片 URL / 文档 URL） |
| `extra_input.use_thinking` | boolean | 否 | 是否开启深度思考，与输入框「深度思考」开关一致 |
| `extra_input.session_config_patch.model_options[]` | object[] | 否 | 出图通道；`platform` 为通道 id（数字）或名称（字符串），页面从 Apollo 配置 `model_options` 取，过滤掉「垂直模型」「私有模型」 |
| `extra_input.knowledge_mount` | object | 否 | 挂载知识库。键是库分类 id（`0`、`1`），值为 `{folder_ids: string[], file_ids: string[]}`；只有勾选了文件夹/文件才会出现，全部为空时该字段不传 |

#### `/chat` 的 SSE 事件（前端逐条 apply，共 9 类）

流格式是标准的 `data: {json}\n`，逐行解析后按 `type` 分发：

| `type` | 承载字段 | 前端动作 |
| --- | --- | --- |
| `session_info` | `session_id`、`session_title` | 首次对话拿到新会话 id，写入 localStorage 并替换 URL query |
| `run_info` | `run_id` | 记录本次 run id，用于「停止」和断线恢复 |
| `tips` | `content` / `message` / `text` | 头部提示条（正文为空时才显示） |
| `tips2` | `content` / `message` / `text` | 尾部提示条（工具分析提示） |
| `thinking` | `content` | 追加到推理过程 `reasoning` |
| `tool_call` | `tool_call_id`、`function_name`、`display_name`、`arguments`、`summarize_with_llm`、`frontend_render`、`is_streaming_output` | 新增一个工具调用卡片，`status=1` |
| `tool_result` | `tool_call_id`、`function_name`、`result`、`status`、`result_type`、`render_payload` | 回填工具调用结果；`result_type` 默认 `text`，`frontend_render=true` 时前端自绘 |
| `delta` | `content` | 正文增量追加，最核心的事件 |
| `title` | `session_id`、`title` | 会话标题被服务端改写时同步侧边栏 |
| `error` | `message` / `content` / `error` / `detail` | 置为流错误，跳过历史重载 |
| `stopped` / `committed` / `done` | — | 结束流式状态 |

### 4.3 会话列表/消息的字段含义

`/sessions` 返回项（前端归一化后）：

| 字段 | 含义 |
| --- | --- |
| `session_id` | 会话唯一 id（字符串） |
| `session_title` | 会话标题，未命名为 `untitled` |
| `created_at` / `updated_at` | 创建/更新时间，侧边栏按时间分组 |
| `is_favorite` | 是否收藏（1/0） |
| `thinking_enabled` | 该会话是否开启深度思考（1/0） |
| `raw` | 服务端原始对象，保留其余字段 |

`/messages` 返回的每个轮次（turn）：

| 字段 | 含义 |
| --- | --- |
| `turn_id` | 轮次 id，消息操作用它 |
| `response_content` | 助手回答正文 |
| `reasoning_content` | 思考过程 |
| `tool_calls[]` | 该轮调用的工具列表 |
| `variant_id` / `selected_variant_id` | 候选答案 id / 当前选中 id |
| `variant_no` / `variant_count` | 第几个候选 / 共几个候选 |
| `variants[]` | 候选答案数组 |
| `is_last_turn` | 是否最后一轮（决定能否删除） |
| 用户侧内容 | 由 `request_user_message_format` 判断格式：`json` 则解析为 content parts 数组，否则按字符串；数组里的 `text` 拼成用户文本，`image_url`/`file_url` 转成文件 chip |

### 4.4 临时数据（同页使用）

| 请求类型 | 接口 | 功能 | 入参 |
| --- | --- | --- | --- |
| POST | `https://gateway.iyw.cn/ai-application/api/tempData/getAndDelete` | 从其他页面带来的临时草稿只读一次并删除 | Body：`data_id`；返回 `data.text`、`data.files[]`，前端去重（`sessionStorage` 记录 `aiAgent:paramsId:handled:{id}`）后填入输入框 |
