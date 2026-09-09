# 客户需求、设计稿、图片搜索与提示词

资料来源：用户提供的 2026-09-09 接口详细版（线上前端包 + 部分登录态实测）。表中的“实测”是原资料的证据标记，不表示本次已调用。未明确的必填字段、类型、枚举、完整表单仍须结合实际页面请求确认。

调用入口：`fetch_iyw_url`。先读 [HTTP 约定](iyw-http.md)，每次提供 `description`；检查 HTTP 信封及业务 `body.code`。这里只提供接口资料，不创建 capability_id，不走 search/read/invoke。检索其他业务见 [接口索引](iyw-api-index.md)。

路径规则：完整 URL 为 `https://gateway.iyw.cn/` + 表中服务路径；开头 `/` 只表示路径根，不改变域名。保留大小写和接口原始拼写。简称接口沿用该行左侧路径的父目录。GET 的参数放 `query`，POST 的参数放 `body`。

检索词：客户需求、需求发布单、客户趋势、设计方案、设计稿、采购目录、以图搜图、图片描述、提示词收藏、customerRequirement、designScheme。

流程：以图搜图返回的 `score` 是相关性，不能当授权证明。先列客户需求，再读详情；生成 PDF 后检查任务或文档 URL。`designScheme/generateImage` 由 `generate_iyw_image(type=scheme-generate)` 执行，列表、详情、删除、结果查询仍用 fetch。`ai-chat/api/chat/describe` 是原业务描述接口，可用 fetch；主机图像理解工具的路由仍按原网关规则。

```json
{"description":"查询客户需求列表","url":"https://gateway.iyw.cn/ai-chat/api/customerRequirement/getList","body":{"keywords":"","page":1,"pageSize":20}}
```

`classify` 可从当前页面选择；已确认常见类目：1 正版图案、2 AI 稿、4 实拍图、5 IP、11 销售画册、51 趋势、52 展会报告。不要额外排除用户需要的类别。

## 接口与参数

## 四、AI 对话与客户需求（`ai-chat`）

**全部 POST**。

| 请求类型 | 接口 | 功能 | 入参 | 参数含义 |
| --- | --- | --- | --- | --- |
| POST | `ai-chat/api/imageSearch/search` | 以图搜图 | `searchImage`、`searchText`、`classify[]`、`exceptClassify[]`、`page`、`pageSize` | `searchImage` 图片 URL（前端拼 `?x-tos-process=image/resize,w_600/format,png` 压缩）；`searchText` 文本辅助；`exceptClassify:[3,51,52]` 排除类目 ✅ 实测返回 `score` |
| POST | `ai-chat/api/chat/describe` | 图片描述 | `imageUrl` | AI 描述文本 |
| POST | `ai-chat/api/customerRequirement/add` | 新增客户需求 | `content`、`images[]`、`customerId` | |
| POST | `ai-chat/api/customerRequirement/getList` | 需求列表 | `keywords`、`trenderType`、`page`、`pageSize` | ✅ |
| POST | `ai-chat/api/customerRequirement/detail` | 需求详情 | `id` | |
| POST | `ai-chat/api/customerRequirement/deleteRequirement` | 删除需求 | `id` | |
| POST | `ai-chat/api/customerRequirement/adoptImage` | 采纳图片 | `requirementId`、`imageId` | |
| POST | `ai-chat/api/customerRequirement/setClose` | 关闭需求 | `id` | |
| POST | `ai-chat/api/customerRequirementPublish/getList` | 发布单列表 | 分页 | |
| POST | `ai-chat/api/customerRequirementPublish/detail` | 发布单详情 | `id` | |
| POST | `ai-chat/api/customerRequirementPublish/editTitle` | 改标题 | `id`、`title` | |
| POST | `ai-chat/api/customerRequirementPublish/generatePDF` | 生成需求 PDF | `id` | |
| POST | `ai-chat/api/customerRequirementPublish/delete` / `deleteImages` | 删除发布单/图片 | `id`、`imageIds[]` | |
| POST | `ai-chat/api/customerTrender/add` | 采集客户趋势 | `themeType`、`trenderType`、`fileUrl[]` | `themeType` 主题类型；`trenderType` 趋势类型 ✅ |
| POST | `ai-chat/api/customerTrender/trenderList` | 客户趋势列表 | `keywords`、`trenderType`、`page`、`pageSize` | ✅ |
| POST | `ai-chat/api/customerTrender/getProcess` | 处理进度 | `id` | |
| POST | `ai-chat/api/customerTrender/setStatus` | 改状态 | `id`、`status` | |
| POST | `ai-chat/api/customerTrender/deleteTrender` | 删除趋势 | `id` | |
| POST | `ai-chat/api/designScheme/list` | 设计稿列表 | 分页 | |
| POST | `ai-chat/api/designScheme/detail` | 设计稿详情 | `id` | |
| POST | `ai-chat/api/designScheme/generateImage` | 设计稿生图 | `schemeId`、`prompt` | |
| POST | `ai-chat/api/designScheme/deleteImage` | 删除设计图 | `imageId` | |
| POST | `ai-chat/api/designScheme/searchGenerateResult` | 检索生成结果 | `taskId` | |
| POST | `ai-chat/api/dialogue/list` / `delete` | 会话列表 / 删除 | 分页 / `id` | |
| POST | `ai-chat/api/favourite/list` / `set` | 收藏列表 / 收藏 | 分页 / `id`、`type` | |
| POST | `ai-chat/api/favourite/addPrompt` / `deletePrompt` / `getPromptList` | 提示词收藏 | `prompt` / `id` / 分页 | |
| POST | `ai-chat/api/prompt/refine` / `refine/v2` | 提示词优化 | `prompt`、`models` | |
| POST | `ai-chat/api/trender/GetThemeList` | 趋势主题列表 | `keywords`、`orderBy`、`market`、`pageIndex`、`pageSize` | `market`：0→-1 表示全部 ✅ |
| POST | `ai-chat/api/trender/OptimizePrompt` | 趋势提示词优化 | `prompt` | |
| POST | `ai-chat/api/procurementCatalog/detail` | 采购目录详情 | `id` | |
