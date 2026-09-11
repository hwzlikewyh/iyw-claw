# 知识库文件夹、文件、切片、附件与容量

来源：2026-09-10《爱原物接口文档·分层详版（补充篇）》；前端反解和原采集者实测标记，不表示本次已调用。只读取当前任务相关小节；其他领域见 [接口索引](iyw-api-index.md)，冲突和认证/签名边界见 [调用约定补充](iyw-api-access-contracts.md)。

管理业务统一用 fetch_iyw_url；原有 search_iyw_knowledge 保留正文检索专用能力。先选择 category 0/1，再从 parentId:"0" 查根目录；文件夹/文件 ID 使用实际返回字符串。删除文件夹必须依照 delete-plan 返回内容和用户选择的 delete_all/move_files 执行，completed 与 syncFailedFiles 都要检查。

通用上传工具仍限制 50 MiB；还要遵守知识库更小的限制，例如图片 8 MB、Markdown/文本 10 MB。上传后可以使用返回 URL 调 files/create，至少提供实际 fileUrl/uri/tosUrl 之一，不能猜 bucket、tosPath 或内部 URI。files/upload-token 只记录为主机凭证流程，不让代理读出 STS。知识库不接受 URL 时应明确缺少所需服务支持。

files/create、batch-move、batch-delete、folders/delete-tree 可用 timeout_seconds:600。批量状态刷新每次 <=10 文件，间隔5秒，完成/失败即停止正常轮询；失败项最多按文档额外 forceRefresh 一次，不自动重新上传或创建。

```json
{"description":"查询个人知识库目录","url":"https://gateway.iyw.cn/ai-agent-new/api/knowledge/folders/list","body":{"category":0,"parentId":"0"}}
```

```json
{"description":"查询知识库文件","url":"https://gateway.iyw.cn/ai-agent-new/api/knowledge/files/list","body":{"category":0,"folderId":"0","includeChildren":false,"sortBy":"time","sortOrder":"desc","page":1,"pageSize":20}}
```

## 来源详细资料

## 五、知识库全套（`ai-agent-new/api/knowledge`，23 个接口）

`ai.iyw.cn` 的「知识库」页面（左侧库分类 0/1，即「我的知识库 / 企业知识库」两套）。源码位置：`ai_8624.de297001.js`（页面）、`ai_7323.d6740932.js`、`ai_8007.9e20690a.js`（内嵌版）。**全部 POST**，路径同样有两套前缀：

| 前缀 | 环境 |
| --- | --- |
| `/ai-agent-new/api/knowledge` | 线上默认 |
| `/api/knowledge` | 开发模式（Cookie `agent_new_dev_token`），带 `token` + `tokenInfo` |

### 5.0 公共约定

- 所有请求体都会自动补一个 `category` 字段：`0` = 我的知识库，`1` = 企业知识库（页面按 `libraryCategory` 注入）。
- 文件夹 id 用字符串，根/默认文件夹固定是 `"0"`。
- 文件处理状态 `docProcessStatus`：`-1` 未知、`0` 处理完成、`1` 处理中、`2` 处理失败、`3`/`6` 处理中；同步状态另有 `0` 待同步、`1` 同步中、`2` 已同步、`3` 同步失败。
- 容量上限来自权益码 `I19`：`quotaSize = I19.remain × 1024³` 字节；`/storage/stats` 返回已用情况。
- 上传分两步：先 `/files/upload-token` 拿 TOS STS 凭证 → 客户端直传对象存储（分片 10 MB、并发 3）→ 再用 `/files/create` 提交元数据。
- 支持的文件与单文件大小上限（源码常量表）：PDF 1 GB；doc/docx/epub/html/htm 350 MB；ppt/pptx 200 MB；md/txt/vtt 10 MB；mp4/mkv/avi/mov/wmv 512 MB；图片（jpg/jpeg/png/webp/bmp/tiff/ico/dib/icns/sgi/jp2）8 MB，最短边 ≥10px、最长边 ≤6000px、宽高比 0.01~100。

### 5.1 文件夹（8 个接口）

| 请求类型 | 接口 | 功能 | 入参 | 参数含义 |
| --- | --- | --- | --- | --- |
| POST | `/folders/list` | 当前层文件夹列表 | `category`、`parentId`、`folderName`（可选搜索词） | `parentId` 传 `"0"` 表示根；`folderName` 非空时按名称过滤；返回 `data.list[]` |
| POST | `/folders/tree` | 整库文件夹树 | `category` | 返回 `data.folderTree[]`，用于「移动文件」下拉；前端拍平成 `{id,label}` 选项 |
| POST | `/folders/path` | 取某文件夹的完整路径（面包屑） | `category`、`folderId` | 返回 `data.path[]`，顺序为根 → 当前；前端把第一项改写成「知识库」标题 |
| POST | `/folders/create` | 新建文件夹 | `category`、`folderName`、`parentId` | `folderName` 前端校验：不能为空、≤64 字符；根目录下不允许再建一级（默认文件夹限制） |
| POST | `/folders/update` | 重命名文件夹 | `category`、`folderId`、`folderName` | 新名与旧名相同则直接返回不请求 |
| POST | `/folders/delete` | 删除空文件夹 | `category`、`folderId` | 默认文件夹（id 0）禁止删除；若返回 `data.empty === false` 说明里面有文件，必须改走 `/folders/delete-plan` |
| POST | `/folders/delete-plan` | 删除前预检 | `category`、`folderId` | 返回该文件夹统计：`folderId`、`totalFileCount`、子文件夹/文件信息；前端据此决定「移动文件」还是「连文件一起删」 |
| POST | `/folders/delete-tree` | 递归删除文件夹（含内容） | `category`、`folderId`、`mode`、`targetFolderId` | `mode`: `delete_all`（连文件一起删）/ `move_files`（先把文件搬到 `targetFolderId` 再删）；`targetFolderId` 仅在 `move_files` 时必填；**超时 600 秒**；返回 `completed`、`syncFailedFiles[]`、`message` |

### 5.2 文件（9 个接口）

| 请求类型 | 接口 | 功能 | 入参 | 参数含义 |
| --- | --- | --- | --- | --- |
| POST | `/files/list` | 文件列表（分页） | `category`、`folderId`、`searchText`、`includeChildren`、`sortBy`、`sortOrder`、`fileCategories[]`、`page`、`pageSize` | `includeChildren` 页面固定 `false`（只列当前层）；`sortBy` 取 `default`（默认排序）/`name`（名称）/`time`（最近更新）；`sortOrder` 为 `asc`/`desc`；`fileCategories` 取值 `word/excel/pdf/ppt/image/video/text/markdown/html/other`，不传等于全部；`pageSize` 默认 50；返回 `data.list[]`，前端按 `list.length >= pageSize` 判断还有下一页 |
| POST | `/files/create` | 新建文件记录（上传完成后调用） | `category`、`fileName`、`originalFileName`、`fileUrl`、`uri`、`tosUrl`、`tosPath`、`mimeType`、`fileSize`、`folderId`、`fileType`、`tagIds[]`、`categoryIds[]` | `fileUrl` 知识库可读 URL、`uri` 内网可读 URI、`tosUrl` 形如 `tos://{bucket}/{key}`，三者至少填一个；`fileType` 默认 0；**超时 600 秒** |
| POST | `/files/update` | 重命名 / 移动单个文件 | `category`、`fileId`、`fileName`、`folderId` | 页面「编辑」弹窗同时改这两个字段 |
| POST | `/files/delete` | 删除单个文件 | `category`、`fileId` | 删除后刷新列表、关闭详情 |
| POST | `/files/batch-move` | 批量移动文件 | `category`、`fileIds[]`、`folderId`（目标） | **超时 600 秒**；返回 `movedCount`、`failedCount`，前端按失败数给不同提示 |
| POST | `/files/batch-delete` | 批量删除文件 | `category`、`fileIds[]` | **超时 600 秒**；返回 `deletedCount`、`failedCount` |
| POST | `/files/refresh-status` | 刷新单个文件的处理状态 | `category`、`fileId` | 用于手动点「刷新状态」 |
| POST | `/files/batch-refresh-status` | 批量刷新处理状态（轮询用） | `category`、`fileIds[]`、`forceRefresh` | 页面每 5 秒对「非 0 且非 2」的文件轮询一次，`forceRefresh:false`；对已失败文件只补刷一次，`forceRefresh:true`，每批最多 10 个 |
| POST | `/files/upload-token` | 获取 TOS 上传凭证 | `category`、`folderId` | 返回 `accessKeyId`、`accessKeySecret`、`stsToken`(`securityToken`)、`region`、`endpoint`、`bucket`、`objectPrefix`；`objectPrefix` 是授权目录，客户端拼接对象 key 时会校验前缀，越界直接抛错 |

### 5.3 切片（向量片段，5 个接口）

「切片」= 文档被拆成的一条条可检索片段。

| 请求类型 | 接口 | 功能 | 入参 | 参数含义 |
| --- | --- | --- | --- | --- |
| POST | `/points/list` | 切片列表 | `category`、`fileId`、`offset`、`limit`、`getAttachmentLink` | 页面固定 `offset:0`、`limit:100`、`getAttachmentLink:true`；返回 `data.list[]` |
| POST | `/points/create` | 新增切片 | `category`、`fileId`、`chunkType`、`chunkTitle`、`content`、`question` | `chunkType` 固定 `"text"`；`content` 与 `question` 至少填一个 |
| POST | `/points/info` | 切片详情 | `category`、`fileId`、`pointId`、`getAttachmentLink` | 返回 `data.point`，前端直接格式化 JSON 展示 |
| POST | `/points/update` | 修改切片 | `category`、`fileId`、`pointId`、`chunkTitle`、`content`、`question` | 与 create 相比多了 `pointId` |
| POST | `/points/delete` | 删除切片 | `category`、`fileId`、`pointId` | 二次确认后调用 |

### 5.4 检索、附件与容量（4 个接口）

| 请求类型 | 接口 | 功能 | 入参 | 参数含义 |
| --- | --- | --- | --- | --- |
| POST | `/search` | 语义检索 | `category`、`query`、`limit`、`denseWeight`、`folderId`（可选） | `query` 检索词（≤500 字）；`limit` 返回切片条数，前端限制 1~20，默认 10；`denseWeight` 稠密检索权重 0.2~1，默认 0.5，越大越偏语义、越小越偏关键词；只有打开「只检索当前文件夹」且当前在文件夹内时才带 `folderId` |
| POST | `/attachments/refresh` | 同步刷新某个文件的附件链接 | `category`、`fileId` | 返回同列表项结构 |
| POST | `/attachments/refresh-stream` | **流式**刷新附件链接 | `category` 等 | 用 `fetch` + `text/event-stream`，逐块解析 `event: item / item_error / done`；回调 `onItem`、`onItemError`、`onDone`、`onError` |
| POST | `/storage/stats` | 存储统计 | `category` | 返回 `data.total` 或 `data.current`，字段兼容 `fileCount`/`file_count`、`fileSizeBytes`/`file_size_bytes`/`fileSize`/`file_size`；配额另由 `I19` 计算 |
