# 图片任务管理、素材、收藏与 PDF

新增 [套图版本](iyw-api-product-kits.md)、[批量中心](iyw-api-batch-center.md)、[PDF 全流程](iyw-api-pdf-workflows.md) 按需读取。补充给出了蒙版服务域，但区域参数仍不完整；不能继续把旧文档的“域未知”当当前事实。

调用入口统一为 `fetch_iyw_url`，先读 [HTTP 约定](iyw-http.md)。生成或处理图片使用 [图片工具](iyw-image-tools.md)，不通过 fetch 重复提交。其他业务见 [索引](iyw-api-index.md)。

检索词：生成记录、任务详情、taskId、回收站、删除图片、收藏、素材、提示词案例、PDF 购物车、PDF 导出、桌面授权、文档解析。

## 路径与任务查询

下表 URL 为 `https://gateway.iyw.cn/ai-application/api/` 加对应路径。
Commerce 任务查 `commerce/getCommerceTaskDetail`；分身和 microModel 任务查 `microModel/GetDetails`；设计稿任务查 `https://gateway.iyw.cn/ai-chat/api/designScheme/searchGenerateResult`。使用创建响应原样返回的 taskId，不能混用查询接口。`detele`、`getMateial` 是原始拼写。

```json
{"description":"查询图片生成任务进度","url":"https://gateway.iyw.cn/ai-application/api/commerce/getCommerceTaskDetail","body":{"taskId":"替换为创建响应中的真实任务ID"}}
```

`status/process`、失败消息、图片 URL 均从实际返回读取；原实现 `process=10` 成功、20/30 失败、0/1 排队。其他值或形状不得推断已成功。超时后先查询原任务，不重新提交扣点操作。分页资料未给字段名时先核对当前页面。

| 方法 | 相对接口 | 用途 | 入参 |
| --- | --- | --- | --- |
| POST | `commerce/getCommerceTasks` | 任务列表 | `page`、`pageSize`、`status` | 查询 |
| POST | `commerce/getCommerceTaskDetail` | 任务详情 | `taskId` | 查询 |
| POST | `commerce/getCommerceTaskRecycleList` | 回收站 | 分页 | 查询 |
| POST | `commerce/removeTaskOrImage` | 删除任务/图片 | `taskId`、`imageId` | — |
| POST | `commerce/addCommerceCollect` / `removeCommerceCollect` | 收藏 / 取消 | `imageId` | — |
| POST | `microModel/GetList` | 生成记录 | `page`、`pageSize`、`state` |
| POST | `microModel/GetDetails` | 任务详情 | `taskId` |
| POST | `microModel/Recycle` | 回收站 | 分页 |
| POST | `microModel/task/detele` | 删除任务 | `taskId`（接口名拼写为 detele） |
| POST | `microModel/chat/history` | 对话历史 | `chatId`、分页 |
| POST | `microModel/createNewChat` | 新建会话 | `state` |
| POST | `microModel/reset` | 重置会话 | `chatId` |
| POST | `microModel/collects` / `userCollections` | 收藏列表 | 分页、`type` |
| POST | `microModel/addCollect` / `removeCollect` / `deleteCollectImg` | 收藏增删 | `imageId` |
| POST | `microModel/PreSignedUrl` | 上传预签名 | `fileName`、`contentType` |
| GET | `microModel/stsToken` | STS 临时凭证 | 无 |
| POST | `microModel/getMateial` | 取素材 | `materialId` |
| POST | `microModel/RequirementImageDetail` | 需求图详情 | `imageId` |
| POST | `promptCase/getAllCase` | 提示词案例库 | 无（实测 143 KB） |

`PreSignedUrl`、`stsToken` 是上传内部凭证步骤，直接使用 `upload_iyw_file`，不让代理获得签名/STS。现有已确认预签名入参是 `objectKey`，原文 `fileName/contentType` 摘要不能替换现有协议。

## 应用辅助接口

以下原始表保留检索覆盖。完整 `ai-application/...` 路径从 gateway origin 开始；只给 `/api/...` 的行缺少明确服务域，需从页面确认 URL，不能按名称猜前缀。

### 3.4 其他 AI 应用接口

| 请求类型 | 接口 | 功能 | 入参 |
| --- | --- | --- | --- |
| POST | `ai-application/api/desktopAuth/grant` | 桌面端授权 | `code`/`token` |
| POST | `ai-application/api/desktopAuth/session` | 会话校验 | 无 |
| POST | `ai-application/api/geoExperience/apply` | 地理体验申请 | 表单字段 |
| POST | `ai-application/api/image2pdfCart/getList` / `batchAdd` / `batchDelete` | PDF 购物车 | `imageIds[]` |
| POST | `ai-application/api/image2pdfGenerateRecord/getList` | PDF 生成记录 | 分页 |
| POST | `ai-application/faddish/generate` | 造花生成 | `prompt`、`imageUrls` |
| POST | `/api/generate_mask` | 生成蒙版 | `imageUrl`、`points` |
| POST | `/api/generate_pdf/query-image-edit` / `query-pdf-export` | PDF 编辑/导出查询 | `taskId` |


`ai-application/faddish/generate` 由图片工具 `type=faddish` 执行。补充已确认蒙版地址为 `https://ai.iyw.cn/agent/api/generate_mask`，但区域参数结构未完整给出，当前不猜测执行；蒙版可使用已有文件并上传。桌面授权由主机账号流程处理。PDF 查询与购物车、申请等剩余业务均用 fetch。文件/表单不能自动推断成 multipart；fetch 支持 JSON、URL-encoded form、text，不支持 multipart 二进制。

来源：2026-09-09 用户接口文档；实测标记和字段不完整的限制见 [索引](iyw-api-index.md)。
