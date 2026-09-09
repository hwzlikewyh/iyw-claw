# PDF 购物车、模板、生成与状态

来源：2026-09-10《爱原物接口文档·分层详版（补充篇）》；前端反解和原采集者实测标记，不表示本次已调用。只读取当前任务相关小节；其他领域见 [接口索引](iyw-api-index.md)，冲突和认证/签名边界见 [调用约定补充](iyw-api-access-contracts.md)。

PDF 文档生成、模板、购物车、进度全部通过 fetch_iyw_url；图片处理例外由图片工具负责。购物车 batchAdd/batchDelete 使用补充明确的 imageUrls 数组；不把旧 imageIds 示例直接混用。chat PDF 使用 layoutType:chat 和实际 messages，按协议剔除 items/recordId。

pdf-service 的导出 POST 传 html；拿到真实 task_id 后按已确认查询方法轮询 /api/v1/pdf/task/{task_id}，完成后使用 /download/{task_id}。原资料没有标明这两个路径的方法，不猜测写方法。ai.iyw.cn 的 query-pdf-export/query-image-edit 明确为 GET，task_id 放 query。

蒙版地址已明确为 https://ai.iyw.cn/agent/api/generate_mask，但“区域”字段和坐标结构仍未完整给出；图片 MCP 尚不猜测生成蒙版，使用已有蒙版或核对实际页面契约。WS 不通过 fetch；SSE 返回完整文本。大型 PDF/ZIP 仍受 2 MiB 响应上限限制。

## 来源详细资料

### 8.4 图片转 PDF（两套实现）

| 请求类型 | 接口 | 功能 | 入参 |
| --- | --- | --- | --- |
| POST | `/ai-application/api/image2pdfCart/getList` | PDF 购物车列表 | 空对象 `{}` |
| POST | `/ai-application/api/image2pdfCart/batchAdd` | 批量加入购物车 | `imageUrls[]` |
| POST | `/ai-application/api/image2pdfCart/batchDelete` | 批量移除 | `imageUrls[]` |
| POST | `/ai-application/api/image2pdfGenerateRecord/generate` | 生成 PDF | `layoutType`、`messages[]` 等；`layoutType` 固定 `chat`，会剔除 `items`、`recordId` |
| POST | `/ai-application/api/image2pdfGenerateRecord/getProgress` | 查询进度 | `recordId` |
| POST | `/ai-application/api/image2pdfGenerateRecord/getDetail` | 生成结果详情 | `recordId` |
| POST | `/ai-application/api/image2pdfGenerateRecord/getList` | 生成记录列表 | 分页 |
| POST | `/ai-application/api/image2pdfUserTemplate/saveTemplate` | 保存为我的模板 | `recordId`、`name`、`html` |
| POST | `/ai-application/api/image2pdfUserTemplate/getList` | 我的模板列表 | 分页 |
| POST | `/ai-application/api/image2pdfUserTemplate/getDetail` | 模板详情 | `recordId` |
| POST | `/ai-application/api/image2pdfUserTemplate/delete` | 删除模板 | `recordId` |
| POST | `https://pdf-service.iyw.cn/api/v1/pdf/export` | PDF 导出服务（另一套） | `{ html }`；返回 `task_id`，轮询 `/api/v1/pdf/task/{task_id}`（间隔 2 秒、最多 150 次），完成取 `/api/v1/pdf/download/{task_id}` |
| POST | `https://ai.iyw.cn/agent/api/generate_mask` | 生成蒙版 | `imageUrls[]`、区域 |
| GET | `https://ai.iyw.cn/agent/api/generate_pdf/query-pdf-export` | 查询 PDF 导出任务 | `task_id` |
| GET | `https://ai.iyw.cn/agent/api/generate_pdf/query-image-edit` | 查询图片编辑任务 | `task_id` |
| WS | `wss://ai.iyw.cn/agent/api/generate_pdf/ws/generate-pdf` | PDF 生成进度推送 | 发 `{action:"generate_pdf", imageGroup, designData, messages, language}` |
