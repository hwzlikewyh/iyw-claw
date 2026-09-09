# 智能体与应用会话

资料来源：用户提供的 2026-09-09 接口详细版（线上前端包 + 部分登录态实测）。表中的“实测”是原资料的证据标记，不表示本次已调用。未明确的必填字段、类型、枚举、完整表单仍须结合实际页面请求确认。

调用入口：`fetch_iyw_url`。先读 [HTTP 约定](iyw-http.md)，每次提供 `description`；检查 HTTP 信封及业务 `body.code`。这里只提供接口资料，不创建 capability_id，不走 search/read/invoke。检索其他业务见 [接口索引](iyw-api-index.md)。

路径规则：完整 URL 为 `https://gateway.iyw.cn/` + 表中服务路径；开头 `/` 只表示路径根，不改变域名。保留大小写和接口原始拼写。简称接口沿用该行左侧路径的父目录。GET 的参数放 `query`，POST 的参数放 `body`。

检索词：原助理、本体、agent、conversation、session、聊天、历史、继续生成、停止、重命名、收藏。

流程：先按 `agent_id` 列表定位真实 `session_id`，再读详情；更新/删除使用返回的 ID。`MessageID`、`conversation_id` 与 `session_id` 不是可互换字段。`chat` 为流式接口，fetch 只在流结束后返回完整文本，60 秒/2 MiB 限制仍有效；超时不能当作未发起对话。优先查已有会话/运行结果，不自动再次发送。

```json
{"description":"查询原助理会话列表","url":"https://gateway.iyw.cn/ai-agent/api/conversation/list","body":{"agent_id":1,"page":1,"page_size":20}}
```

## 接口与参数

## 一、原助理智能体会话（`ai-agent`）

原助理 / 本体智能体的对话能力，**全部 POST**，参数为**下划线命名**。

| 请求类型 | 接口 | 功能 | 入参 | 参数含义 |
| --- | --- | --- | --- | --- |
| POST | `/ai-agent/api/conversation/chat` | 发起一轮对话（流式） | `agent_id`、`session_id`、`text`、`files`、`extra_input` | `agent_id` 智能体编号（1=本体智能体）；`session_id` 会话 ID，新建可不传；`text` 用户输入；`files` 附件数组；`extra_input` 额外上下文（工作区/工具配置） |
| POST | `/ai-agent/api/conversation/chat_continue` | 继续生成 / 重新生成 | `agent_id`、`session_id`、`run_id` | `run_id` 上一轮运行 ID，用于断点续答 |
| POST | `/ai-agent/api/conversation/stop_message` | 停止正在生成的消息 | `MessageID` | `MessageID` 目标消息 ID |
| POST | `/ai-agent/api/conversation/list` | 会话列表 | `agent_id`、`page`、`page_size` | 按智能体分页拉历史 |
| POST | `/ai-agent/api/conversation/detail` | 会话详情 | `agent_id`、`session_id` | 返回消息与轮次 |
| POST | `/ai-agent/api/conversation/delete` | 删除会话 | `session_id` | 逻辑删除 |
| POST | `/ai-agent/api/conversation/update_conversation` | 重命名 / 更新会话 | `agent_id`、`session_id`、`title` | 改标题等元信息 |
| POST | `/ai-agent/api/conversation/setFavourite` | 收藏 / 取消收藏 | `conversation_id`、`favourite` | `favourite` 0↔1 切换 |
| POST | `/ai-agent/api/conversation/getSaleData` | 会话用量/销售数据 | `agent_id`、`session_id` | 统计用 |
| POST | `/ai-application/api/conversation/chat` | 应用侧对话 | 同上 | 同名，走应用前缀 |
| GET | `/ai-agent-new/api/agent/favorites` | 收藏的智能体 | 无 | 列出已收藏 |

**证据片段**（实际调用代码）：

```js
{agent_id:this.agentId, session_id:this.activeSessionId, text:t||null, files:i, extra_input:l}
{agent_id:this.agentId, page:t, page_size:this.sessionPageSize}
{agent_id:this.agentId, session_id:i, turn_id:s, variant_id:a}
{conversation_id:t.conversation_id, favourite:0==t.is_favourite?1:0}
```
