# 技能、专家包与插件的上传、发布和版本管理

先读 [市场入口](capability-market.md) 确定对象和受众。API 前缀为 Fusion 基础地址；用户请求使用当前登录 token，由宿主工具注入或客户端会话取得。技能、专家包及 v1/v2 插件统一走逐文件 TOS 直传，Fusion 只接收文件清单、上传 ID 和元数据。不要把通用 upload_iyw_file 的 URL 当作市场上传记录。

用户 v2 插件与其他用户包使用相同接口，无需 admin-token。用户包保留 publisherType=user；official 标识仍由管理员维护。登录失效时恢复用户登录，不索要管理员令牌、不改用占位令牌。连接器目录声明和制品另见 [连接器用户发布](connector-publishing.md)。

## Agent / CLI 直接上传目录

无需操作文件夹选择框。JSON 请求用 `fetch_iyw_url` 调本页 `/skills/uploads/*`，宿主自动注入登录态；TOS PUT 使用本地执行工具与下列脚本（Node.js 22+），不把平台 token 传给脚本。

1. `node <本skill目录>/scripts/market-direct-upload.mjs manifest <插件或技能绝对目录>`：输出真实 path、size、sha256、mimeType 清单，合并到初始化元数据。目录根就是包根，不携带外层目录名；包含文件须事先整理好，脚本不擅自忽略随包依赖。
2. 按返回 path 映射文件，逐项 `/skills/uploads/url`。只将响应 `data`（url、method、headers）写入本地临时 JSON，可附加原清单 size、sha256 防止文件变化。预签名文件属于短时凭据，不输出到报告或登记为交付物。
3. `node <本skill目录>/scripts/market-direct-upload.mjs put <该文件绝对路径> <临时JSON路径>`：文件原始字节直接传 TOS，不经过 Fusion。不将 Base64 或 multipart 交给 Fusion。
4. 全部成功后 complete、detail 和制品状态核验；按原 uploadId 恢复未知结果。最后清理本轮预签名临时文件。若本轮没有本地执行能力，可使用宿主已有发布器，它也逐文件直传 TOS；两条都不可用时才报告具体缺失能力。

## 准备目录与元数据

独立技能目录：

```text
my-skill/
  SKILL.md
  references/usage.md
  scripts/run.py
```

上传路径是 SKILL.md、references/usage.md 等，不包含 my-skill/ 这一层。SKILL.md 使用合法 name／description frontmatter，入口非空且不超过 1 MiB。插件另按 [插件结构](plugin-packaging.md)：v1带两份原生清单，v2禁止两份原生清单与根.mcp.json；不强行在插件根增加假SKILL.md。

| 字段 | 说明 |
| --- | --- |
| slug | 新项目必需，小写字母／数字／中划线，首尾为字母或数字，最长128；插件名称还受自身清单约束 |
| displayName | 新项目必需，去空白后非空，≤255字符 |
| summary | 描述实际功能，≤4000字符 |
| category | 必需，先 GET /skills/categories，复制真实分类值 |
| iconUrl | 可选图标 URL，≤2048字符；不把本地路径当 URL |
| tags | 字符串数组，按实际标签填写 |
| visibility、audience | 按 [分发范围](capability-market.md) 配对；显式声明 audience |
| version | 严格 SemVer，新版本必须高于当前版本；不覆盖原版本 |
| changelog | 本版变化，≤10000字符 |
| packageType | skill／expert／plugin；不传时无依赖推导为skill、有依赖推导为expert |
| dependencies | 精确 [{slug,version}]；普通独立技能留空；专家包必须与实际依赖一致 |
| files | 原始文件清单 path、size、sha256、mimeType |

所有包最多512文件；Skill／Expert 原始总量默认25 MiB。Plugin **逐文件直传**没有应用层固定体积上限，仍受对象存储、容量、精确整数和文件数约束；不能把这个结论套到主机 Base64 或旧 multipart 路径。插件清单自身上限1 MiB。

路径为相对 POSIX 路径，拒绝绝对路径、..、反斜杠、冒号、重名和大小写冲突、符号链接及宿主 .iyw-claw-*.json 状态文件。不要上传凭据、环境实值、私钥、安装缓存或与技能无关的目录。

## 逐文件直传：五个接口

| POST 路径 | 请求体 | 返回／动作 |
| --- | --- | --- |
| /skills/uploads/init | 新项目元数据或 skillId，新版 version，files等 | uploadId、skillId、version、status、fileCount、totalSize、contentSha256、files |
| /skills/uploads/url | {uploadId,fileId} | fileId、url、method、headers、expiresAt |
| /skills/uploads/status | {uploadId} | 状态、createdAt、expiresAt、files[].uploaded |
| /skills/uploads/complete | {uploadId} | 与初始化同形的结果；检查 status |
| /skills/uploads/abort | {uploadId} | 中止结果；不能据此声称已发布版本也被删除 |

### 1. 初始化

这是 **请求结构模板**；size、sha256 必须由真实文件计算，不直接提交尖括号字符串：

```json
{
  "slug":"my-skill",
  "displayName":"我的技能",
  "summary":"整理指定资料并生成带来源的摘要",
  "category":"<categories返回的值>",
  "tags":["资料整理"],
  "visibility":"private",
  "audience":"owner_private",
  "version":"1.0.0",
  "changelog":"首次发布",
  "packageType":"skill",
  "dependencies":[],
  "files":[
    {"path":"SKILL.md","size":123,"sha256":"<原始字节的64位小写SHA-256>","mimeType":"text/markdown"}
  ]
}
```

size=123 仅演示整数类型；按文件真实字节数替换。sha256 不是 Base64，不是压缩后内容，也不是 contentSha256。初始化响应 files[] 使用 id、path、size、sha256、mimeType；按 path 与本地文件建立映射，不按数组位置盲配。

已有项目发新版改用 skillId，不用 id；例如：

```json
{
  "skillId":"<实际市场ID>",
  "version":"1.0.1",
  "changelog":"补充引用格式",
  "packageType":"skill",
  "dependencies":[],
  "files":[{"path":"SKILL.md","size":123,"sha256":"<真实摘要>","mimeType":"text/markdown"}]
}
```

新版上传不替代 /skills/update 的元数据／受众修改。为避免把已有插件误推导成 skill，新版继续传正确 packageType。

### 2. 每个文件申请 URL 并上传

从初始化响应取真实 uploadId 和该文件的 id：

```json
{"uploadId":"<返回的uploadId>","fileId":"<files中该path的id>"}
```

随后按返回 method（当前为 PUT）、url、headers 发送文件原始字节。浏览器只省略其禁止自行设置的 Host／Content-Length，由浏览器计算；其他签名 headers 保留。**不附加平台 token、tokenInfo、admin-token**。不能把 JSON、Base64 字符串或 multipart 文件包作为该对象的内容。

可由客户端实现如下最小请求：

```javascript
async function putFile(target, file) {
  const headers = Object.fromEntries(
    Object.entries(target.headers || {}).filter(
      ([key]) => !/^(host|content-length)$/i.test(key)
    )
  )
  const response = await fetch(target.url, {
    method: target.method,
    headers,
    body: file,
    redirect: "error",
  })
  if (!response.ok) throw new Error("文件直传失败：" + response.status)
}
```

方法／headers 来自服务端，不自行生成签名。大量文件可限制为3并发；这是客户端建议，不是无限并行的授权。CORS 问题由存储服务配置解决，不能通过泄露 STS 或改签名绕过。

### 3. 提交并验证

全部文件成功后 POST /skills/uploads/complete，body 为 {uploadId}。服务端验证大小、摘要、入口、插件清单和依赖；返回 status=ready 才表示源版本已提交。再 POST /skills/detail（id=skillId，version=本次版本）核对当前版本、packageType、受众和摘要。

源版本 ready 与下载 ZIP 制品 ready 分开；制品可能还在 pending／building，签票会返回 SKILL_ARTIFACT_NOT_READY。不要为此重复创建版本。

### 4. 恢复与未知结果

Skill/Expert 上传会话默认30分钟、PUT默认15分钟；Plugin 会话默认24小时、PUT默认2小时，实际以 expiresAt 为准。保存 uploadId、目标 id／version 和清单摘要；不要长期保存凭据或带签名 URL。

恢复时重新选择相同目录，比较每项 path＋size＋sha256；查询 status，只补传 uploaded=false 的文件，且每次重新申请 URL。清单变了就不能假装续传原上传；明确中止旧会话后重新初始化。

complete 超时先查原 uploadId 和版本状态；若已 ready 则完成，不重发创建请求。失效会话用新初始化；用户取消时 abort。已完成会话、失败会话、过期会话按服务端状态处理，不能单靠本地进度判定。

## 旧 multipart 兼容：不用于新上传

以下旧路由仅保留历史客户端兼容。当前宿主发布器已改用上面的直传；新代码、Agent 和大包不要再把文件流、Base64 或 multipart 发到 Fusion：

| 接口 | 表单字段 |
| --- | --- |
| POST /skills | slug、displayName、summary、category、iconUrl、重复 tags、visibility、audience（原始用户API支持）、version、changelog、packageType、dependencies、manifest、文件 parts |
| POST /skills/versions | id、version、changelog、packageType、dependencies、manifest、文件 parts |

manifest 是 JSON 字符串 {"files":[{"path":"SKILL.md","field":"file0"}]}；file0 是同一表单中的一个二进制文件 part 名称。每个清单项对应且只对应一个 part，不允许额外文件。dependencies 也是 JSON 字符串；不是表单中嵌套对象的隐式展开。初次返回 data.skill，新版返回 data.version。

主机JSON发布参数 files:[{path,contentBase64,size}] 只在本地宿主输入中使用；宿主解码后申请上传会话和逐文件 URL，原始字节直接 PUT TOS。最多512文件，size为解码字节数。大插件使用前述目录脚本避免将整个包编码到工具参数；同一版本只创建一次上传会话。

## 专家包与依赖

每项为 {"slug":"<已有技能slug>","version":"<已发布精确SemVer>"}。最多16直接依赖、依赖深度16、安装计划最多64项。拒绝自引用、循环、同一项目多个冲突版本、不可读／缺失依赖。公开市场的整个依赖图都必须 global_market。

## 修改、更新和删除

- POST /skills/update：id 加需要修改的 displayName、summary、category、iconUrl、tags、visibility、audience；省略字段保持原值，iconUrl:null 清空图标。回读 data.skill。
- POST /skills/versions/list：{id}，返回 data.items；用于选择下一 SemVer 和查看状态。
- POST /skills/updates/check：{items:[{id,installedVersion}]}，最多256项；data.items 中 status 为 up_to_date／update_available／unavailable／not_ready。
- POST /skills/delete：{id}；仅用户明确要求市场删除时执行。下架、私有化和本地卸载不是同一操作。

失败时保留本地文件和会话。SKILL_VERSION_CONFLICT／NOT_GREATER 先读版本；FILE_UPLOAD_MISSING／FILE_INTEGRITY_MISMATCH 查状态再补文件；FORBIDDEN 查身份及 canManage；UPLOAD_EXPIRED 重新初始化。请求 outcome 未知时先核对原对象，不换工具重放。

用户 v2 不再因 schemaVersion 被拒绝。FORBIDDEN 时检查当前用户与上传项目归属，不通过降低 schemaVersion、改为 skill 或换管理端点重试。

来源：Fusion docs/skill-market-user-client-integration.md、httpserver/skill/{skill,upload_dto,upload_handlers,request,write_handlers}.go、domain/skill/{validation,manifest,model}.go；宿主 commands/skill_market/{mod,client,types}.rs。限制按调用面区分。
