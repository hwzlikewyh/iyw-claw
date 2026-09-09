# 知识库、自动报告与提示词

资料来源：用户提供的 2026-09-09 接口详细版（线上前端包 + 部分登录态实测）。表中的“实测”是原资料的证据标记，不表示本次已调用。未明确的必填字段、类型、枚举、完整表单仍须结合实际页面请求确认。

调用入口：`fetch_iyw_url`。先读 [HTTP 约定](iyw-http.md)，每次提供 `description`；检查 HTTP 信封及业务 `body.code`。这里只提供接口资料，不创建 capability_id，不走 search/read/invoke。检索其他业务见 [接口索引](iyw-api-index.md)。

路径规则：完整 URL 为 `https://gateway.iyw.cn/` + 表中服务路径；开头 `/` 只表示路径根，不改变域名。保留大小写和接口原始拼写。简称接口沿用该行左侧路径的父目录。GET 的参数放 `query`，POST 的参数放 `body`。

检索词：知识库目录、文件、解析状态、自动报告、趋势推送、分享、口令、prompt、refine。

流程：根目录从 `folderId:0` 开始，先列表取得 ID，再筛选文件。知识正文检索已有 `search_iyw_knowledge` 专用工具；这里的目录/文件/报告接口均走 fetch。报告分享、关分享、重置密码是状态变更，不随查询隐式执行。`runs/query` 的筛选字段在原资料中不完整，应从当前页面确认。

```json
{"description":"查询知识库根目录","url":"https://gateway.iyw.cn/ai-agent-new/api/knowledge/folders/list","body":{"folderId":0,"includeChildren":false,"sortBy":"name","sortOrder":"asc","page":1,"pageSize":20}}
```

```json
{"description":"查询自动报告设置","url":"https://gateway.iyw.cn/ai-agent-new/api/trend-push/auto-report/setting","method":"GET"}
```

## 接口与参数

## 二、趋势推送与知识库（`ai-agent-new`）

| 请求类型 | 接口 | 功能 | 入参 | 参数含义 |
| --- | --- | --- | --- | --- |
| GET | `/ai-agent-new/api/trend-push/auto-report/setting` | 查询自动报告设置 | 无 | 返回 `enabled` |
| POST | `/ai-agent-new/api/trend-push/auto-report/setting` | 开关自动报告 | `enabled` | 布尔 |
| POST | `/ai-agent-new/api/trend-push/runs/query` | 报告记录列表 | 分页 + 筛选 | 历史批次 |
| GET | `/ai-agent-new/api/trend-push/runs/detail` | 报告详情 | `run_id`、`pwd` | `run_id` 批次 ID；`pwd` 分享密码（免登录查看） |
| POST | `/ai-agent-new/api/trend-push/runs/delete` | 删除报告 | `run_id` | |
| POST | `/ai-agent-new/api/trend-push/runs/share` | 生成分享 | `run_id` | 返回链接/口令 |
| POST | `/ai-agent-new/api/trend-push/runs/share/disable` | 关闭分享 | `run_id` | 使链接失效 |
| POST | `/ai-agent-new/api/trend-push/runs/share/reset` | 重置分享密码 | `run_id`、`sharePassword`、`classify` | `sharePassword` 新密码；`classify` 报告分类 |
| POST | `/ai-agent-new/api/knowledge/folders/list` | 知识库文件夹 | `folderId`、`includeChildren`、`category`、`sortBy`、`sortOrder`、`page`、`pageSize` | `folderId=0` 根目录；`sortBy:"name"`；`sortOrder:"asc"` ✅ 实测 |
| POST | `/ai-agent-new/api/knowledge/files/list` | 知识库文件 | `folderId`、`includeChildren`、`docProcessStatus`、`page`、`pageSize` | `docProcessStatus` 解析状态筛选 ✅ 实测 |
| POST | `/ai-agent-new/api/refine-prompt` | 提示词精炼 | `prompt`、`jsonData`、`models` | `models` 指定模型 |
