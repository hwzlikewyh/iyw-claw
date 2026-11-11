# 知识库：导入、整理、检索、引用与维护

本页把文件上传到可被对话可靠使用的全过程串起来。云端完整路径和参数见 [26个接口](iyw-api-knowledge-full.md)，现有客户端选择／上传接口见 [宿主对接](knowledge-host-integration.md)。知识库文档与个人长期记忆不同，个人偏好／项目经验用manage_iyw_memory，正文证据用search_iyw_knowledge。

## 选对库和范围

| 对象 | 使用方法 |
| --- | --- |
| 我的知识库 | category=0 |
| 企业知识库 | category=1；还受当前登录身份和企业权限约束 |
| 默认文件夹 | folderId="0"；不是整库通配符 |
| 整库目录 | folders/tree，只取指定category |
| 文件夹内文件 | files/list，folderId为真实返回ID |
| 当前对话选中文件／文件夹 | 由宿主选择器强制限定检索；不能通过可选参数扩张 |

ID均为字符串；folderId／fileId是业务ID，不能填文件名、文档doc_id、本地路径或点位pointId。category不是插件市场audience，企业库也不是global_market。

只做当前对话范围内问答时，直接检索，不先拉整库文件。只有上传、整理目录、选择文件、排查文档状态才读取目录和列表。

## 导入：对象上传不等于入库

### 1. 明确来源与目标

确认本地文件或已有可读URL、category、真实目标folderId；从folders/tree／list选目标。路径歧义会改变数据归属时需先明确；已知目标不重复询问。

查询storage/stats，并结合现有权益I19核对容量。文件类型／体积按[云接口限制](iyw-api-knowledge-full.md)；客户端补充csv／xlsx／jsonl／bitable的1GiB仅是传输保护，不代表云端承诺解析这些格式。音频当前宿主上传不支持。

### 2. 选择已有上传路径

**原助理内的资料库上传**优先使用现有客户端上传器：浏览器／桌面向宿主暂存分块，宿主取得受限STS并上传，再注册文件。STS不进入模型上下文；详细命令见[宿主上传](knowledge-host-integration.md)。

**Agent已有可读URL路径**：通用upload_iyw_file可把本地文件变成受支持URL，但它不创建知识库记录。需再调用files/create，提供真实fileUrl／uri／tosUrl至少一项，遵守更小的知识库格式限制。

**外部客户端直传**：在已有账户会话中调用files/upload-token，再用官方TOS上传逻辑将字节送到授权objectPrefix下，最后files/create。不得把STS给Agent或用户粘贴到会话，也不能猜bucket、对象key和tos://URI。该流程是客户端集成说明，代理不通过fetch读取凭据。

### 3. 创建知识记录

以下为fetch_iyw_url请求结构示例，尖括号替换为前一步真实结果，fileSize换成真实字节数：

```json
{
  "description":"把已上传文档加入个人知识库",
  "url":"https://gateway.iyw.cn/ai-agent-new/api/knowledge/files/create",
  "method":"POST",
  "timeout_seconds":600,
  "body":{
    "category":0,
    "folderId":"<实际目标文件夹ID>",
    "fileName":"产品规范.pdf",
    "originalFileName":"产品规范.pdf",
    "fileUrl":"<上传返回的知识库可读URL>",
    "mimeType":"application/pdf",
    "fileSize":12345,
    "fileType":0
  }
}
```

只有获得真实tosUrl／tosPath时才附加；不要根据公开URL推导内部存储路径。注册返回成功后保存返回记录，或在同category／folderId下查文件列表核对身份。fileName可能重名，不能用文件名作为唯一证据。

create超时或结果未知，先查原目标列表，并对照真实fileUrl／tosUrl、文件大小、时间等证据，不能再create产生重复知识。宿主原流程会做注册结果核对；无法唯一确认时报告状态未知。

### 4. 等待解析与同步

| docProcessStatus | 含义与动作 |
| --- | --- |
| 0 | 解析完成；继续检查切片／实际检索 |
| 1、3、6 | 处理中；按批刷新 |
| 2 | 解析失败；查看docProcessMessage，正常轮询停止 |
| -1／缺失 | 未知；查询状态，不能当成功或自动重建 |

syncStatus另有0待同步、1同步中、2已同步、3同步失败；它与docProcessStatus不同。不能用syncStatus=2替代解析完成，或反过来。

files/batch-refresh-status每批最多10个，处理中通常5秒一次；完成／失败即停止正常轮询。失败项最多按已记录契约额外forceRefresh=true一次；它只是刷新，不等于重新解析或修复。仍失败保留证据，不能循环上传。

### 5. 验证可检索

检查points/list是否产生相关切片，再用文档中的一个真实术语或明确问题检索，确认命中正确文档。只有文件记录、上传phase=complete或解析状态0，都不足以证明问题能回答。

交付分别报告上传、注册、解析、同步、切片和检索状态；只列实际已验证的步骤。

## 检索与引用

### 优先直接工具

```json
{"query":"产品包装的最小字号和留白要求","limit":5}
```

传给search_iyw_knowledge。有对话选择时省略category／folderId／fileId，由宿主用选中范围执行；目录选择包含子目录，覆盖的重复选择会去重。最多32个选择项。

未设置对话选择，且用户明确指定某库／目录／文件时再填真实category、folderId或fileId：

```json
{"query":"包装标签的尺寸要求","category":1,"folderId":"<实际文件夹ID>","limit":5,"denseWeight":0.5}
```

当前工具允许query=1..4096字符、limit=1..100、denseWeight=0..1；旧云页面资料为query≤500、limit=1..20、denseWeight=0.2..1。它们是不同调用层契约。普通请求取≤500字符、limit=5或10、denseWeight=0.5即可，不把工具宽范围当云端保证。

没有会话选择时，search_iyw_knowledge按默认category=0和提供的过滤执行；没有结果不自动换企业库／清除过滤扩大范围。有会话选择时，原始HTTP /search不应用宿主选择，因此Agent不能换fetch绕过。

### 结果如何使用

宿主规范化结果为count、results；每条可含id、score、content、md_content、chunk_type、doc_id、doc_name、doc_type。选定范围合并结果还可能含category、fusion_score，顶层scope=conversation_selection。

- 先核对doc_name和内容相关性，再摘取支持回答的片段；score不是事实真实性概率。
- 引用真实文档名；返回有point／doc ID时保留供定位。没有页码就不编页码，没有原始可读链接就不伪造下载链接。
- 回答区分原文事实、跨段整理和推断。多个版本冲突时写明来源差异，不能擅自宣布新版或权威版。
- 表格、图片、扫描件或视频切片缺少证据时，说明当前返回内容覆盖不足；不要声称模型已阅读整个原件。
- 文件完整内容不能从少量top-k切片推断；需要“全文归纳”时使用获授权的原文／切片分页，而非只检索一次。

无命中时在同授权范围内改用一个关键术语／同义词，检查目录选择和解析状态。需要扩大范围且不在现有授权中才明确请求用户选择；不把“无证据”回答成“制度没有规定”。

## 整理目录与文件

- 查看tree／path定位真实目录。folders/create传category、parentId、folderName；名称非空≤64字符，保留默认目录限制。
- 改名用folders/update；相同名称无须写请求。
- files/update可改fileName／folderId。移动前确认目标属于同category，不隐式跨个人／企业库。
- batch-move／batch-delete读取movedCount／deletedCount与failedCount；部分成功逐项说明，不把失败数吞掉。
- 删除非空目录先delete-plan；按用户已授权的delete_all或move_files执行delete-tree，move_files必须有真实targetFolderId，检查completed与syncFailedFiles。

仅“丰富知识库”或“整理资料”的请求不隐含删除授权；用户明确要求删除时沿现有预检完成，不额外重复询问相同选择。删除后回读原目录，确认影响，不清理其他库。

## 切片维护

points/list使用fileId、offset、limit、getAttachmentLink；页数不足或空页按实际响应结束。points/info使用准确pointId读取详情；pointId不等于fileId。

新增points/create：chunkType="text"，content与question至少一个非空，chunkTitle按实际含义。修改points/update只变用户要求字段；先读原切片保留未变内容。删除points/delete仅处理用户明确指定片段。

切片编辑是在修改可检索事实。保留原文语义和必要出处，不能把模型推断写成公司制度。改后回读切片并用同问题检索核对是否已生效；索引可能异步，不把首次未命中直接解释为编辑失败。

## 附件、容量和长期维护

附件链接可能到期，先attachments/refresh同步刷新指定fileId；不要反复下载旧URL。refresh-stream的详细请求体在历史资料中不完整，不能仅凭“category等”发起流式写操作；优先已知同步入口。

storage/stats有fileCount／file_count和fileSizeBytes／file_size_bytes等历史字段差异，以实际结构归一化；配额由权益I19另算，不凭已用字节猜总额度。

知识库文档更新、移动、删切片后，重新验证典型检索；对话选择中的文件／目录不可用时重新选择，不能静默取消选择扩大到全库。账号／组织变化时discard旧scope／revision并重新读取。

来源：本skill的知识库云接口资料；当前commands/knowledge/{api,models,library,selection,search,results,upload_models,upload_worker}.rs与builtin_mcp/iyw_knowledge.rs。上传和检索为源码核对，未执行真实用户数据变更。
