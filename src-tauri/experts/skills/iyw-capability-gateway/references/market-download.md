# 市场浏览、下载校验、安装、更新与恢复

基础地址、身份与受众见 [市场入口](capability-market.md)。本页面向技能、专家包和插件消费者；Agent 有插件安装／启用的当前能力时用现有能力，客户端开发者按下列协议实现。下载 ZIP 不等于已安装。

## 用户 API 全表：查询与安装

| 方法与路径 | 参数 | 成功结果／含义 |
| --- | --- | --- |
| GET /skills/list | view=market／mine；page默认1，pageSize默认20且≤100；category、q、visibility、publisherType | data.items、total、page、pageSize；market 按真实受众返回可见市场内容，mine为本人项目 |
| GET /skills/categories | 无 | data.items；用返回值筛选、创建 |
| POST /skills/detail | id，version可选 | data.skill；currentVersion、audience、ownedByMe、canManage等 |
| POST /skills/versions/list | id | data.items；不可变版本和状态 |
| POST /skills/files | id，version可选 | data.tree；递归文件节点，不能当下载地址 |
| POST /skills/install-plan | id，version可选，client可选 | rootSkillId、rootSlug、rootVersion、items；固定根包与全部依赖 |
| POST /skills/artifact-ticket | id、version、artifactId可选、client | url、expiresAt、rangeAllowed、artifact |
| POST /skills/download | id、version可选、client | 旧兼容 ZIP 流或 JSON 错误；新客户端不用它接文件 |
| POST /skills/availability | ids:字符串数组，最多256 | data.items[{id,slug,status}] |
| POST /skills/updates/check | items:[{id,installedVersion}]，最多256 | 当前版本与更新状态；不是安装指令 |
| POST /skills/recovery-plan | id、version、contentSha256、manifestDigest | 已有安装恢复元数据；不用于绕过下载受众限制 |

省略 version 的查询使用当前版；用户选定版本后，后续计划、签票、下载和安装全部固定该版本。所有 ID 保留字符串。列表用实际 total 分页；不能把一个市场分页当全部插件目录。

### Agent 查询示例

fetch_iyw_url 的 method/query/body 按其当前 schema；主机注入凭据：

```json
{"description":"查找可用的办公技能","url":"https://gateway.iyw.cn/iyw-fusion-api/skills/list","method":"GET","query":{"view":"market","page":1,"pageSize":20,"q":"办公"}}
```

```json
{"description":"查看指定插件版本","url":"https://gateway.iyw.cn/iyw-fusion-api/skills/detail","method":"POST","body":{"id":"<返回的市场ID>","version":"<选中的版本>"}}
```

此类 JSON 工具仅查询元数据，不负责把 ZIP 写进技能目录。

## 安装计划与最低宿主版本

```json
{
  "id":"<实际市场ID>",
  "version":"<实际版本>",
  "client":{"clientVersion":"<运行宿主SemVer>","channel":"<实际通道>","target":"windows","arch":"x86_64"}
}
```

clientVersion 必须为运行插件的二进制版本；远程工作区传远程服务端版本，不能传浏览器版本或本地桌面版本。target／arch 由宿主实际平台产生。

新上传交互插件必须声明 minClientVersion，完整 SemVer≤64字节。根或任一依赖有门槛时，缺失／非法／过低版本会返回 SKILL_CLIENT_INCOMPATIBLE；收到后保留旧安装并提示升级宿主，不能当网络错误反复下载。platforms 和 targets 还分别约束操作系统／架构和 Agent。

items 按依赖在前、根包在后排列，包含 skillId、slug、version、packageType、dependencies、plugin、download 等。download 记录 contentSha256、制品大小与相关摘要；新协议还携带制品信息。按实际响应读取，不自行给旧结构补一个虚构 artifactId。

## 票据与直下 ZIP

对每个计划项 POST /skills/artifact-ticket：

```json
{"id":"<item.skillId>","version":"<item.version>","client":{"clientVersion":"<宿主版本>","target":"windows","arch":"x86_64"}}
```

计划明确返回制品 ID 时一并传 artifactId，锁定该制品。票据响应示意：

```json
{
  "url":"<短时对象存储地址>",
  "expiresAt":"<服务端到期时间>",
  "rangeAllowed":true,
  "artifact":{
    "id":"<制品ID>","status":"ready",
    "artifactSize":1234,"artifactSha256":"<ZIP真实SHA-256>",
    "fileName":"<slug-version.zip>","packageKind":"zip"
  }
}
```

示意大小／摘要不能用于真实校验。执行顺序：

1. 比较计划与票据的版本、大小和摘要；不一致停止并重新取得一致计划。
2. GET 返回的短时 URL，不附加平台凭据，不跟随跨源重定向，不记录完整签名 URL。
3. 检查 HTTP 成功和 Content-Type；JSON 错误绝不是 ZIP。流式写临时文件，并累计字节和 ZIP SHA-256。
4. 检查总大小和 artifactSha256；若有签名信息，沿宿主既有可信公钥验证，不能信包内自带的新公钥。
5. 在隔离临时目录解包，拒绝危险路径、符号链接、大小写冲突和超限条目。
6. 校验源文件树 contentSha256、插件清单摘要和实际组件，再提交安装。

源文件树摘要算法：按路径排序，对每文件拼接 path + NUL + size十进制文本 + NUL + fileSHA256 + LF，再计算 SHA-256。文件SHA256为原始字节的摘要；contentSha256 与 ZIP artifactSha256、插件 manifestDigest 是三种不同校验。

票据失效／中断时重新签票；rangeAllowed 只是服务端声明，断点请求还需实际206与 Content-Range 一致，不能无条件追加200响应。大包使用流式文件，不整包 Base64 放进会话。

源版本 ready 但制品 pending／building 时，SKILL_ARTIFACT_NOT_READY 表示打包尚未完成；保留安装意图并稍后检查，不能发布重复版本或转旧流接口绕过新协议。

## 提交安装与接通能力

全部依赖和根包验证成功，才通过现有安装器事务替换正式目录；任一失败保留旧安装。不要直接写 Agent 私有目录或手改安装回执。

独立技能安装到用户选定的受支持 Agent；专家包按计划安装依赖。v1 插件注册技能与连接器；v2 还要完成工作区权限、准备参数、连接器开关和业务账号登录。远程主机使用远程环境、文件和登录态。

Agent 安装插件时：

1. 从市场结果得到真实 skill id、plugin slug／名称及版本。
2. 没有直接安装工具时，用 source=local 搜索“安装插件”，读取返回的完整安装成员，按其 skill_id、version、plugin_name schema 调用；不要从本页猜 capability_id。
3. 安装后如需启用，搜索并读取现有启用成员，按返回 schema 使用真实 plugin_slug。
4. 完成后按用户业务动作搜索插件能力，read后invoke；tools/list／连接成功只能证明发现工具，不能证明外部业务已完成。

安装／授权能力会由宿主处理其实际批准交互；已有明确用户授权不需要代理另加重复确认。工具返回 cancelled／pending／blocked 时如实描述。

## 可用性、更新和回退

availability 的 available／removed／unlisted／inaccessible 分别表示可用、移除、下架、当前身份不可访问。网络失败和未确认状态不能推断为 removed，也不能触发自行删目录。状态处置复用主机安装维护流程。

updates/check 返回 up_to_date／update_available／unavailable／not_ready；展示旧版和候选版，再按新计划重新下载校验。升级不原地替换不可变版本文件；权限／准备声明变化后需重新准备。

回退用主机 plugin_versions_list → plugin_version_inspect → plugin_version_apply，保留 reviewToken 和指定版本；旧安装恢复用 plugin_recovery_inspect → apply。云 recovery-plan 需要旧回执中的准确 version、contentSha256、manifestDigest；缺少证据就报告无法确认，不能改为最新版本或伪造摘要。

## 常见失败

| 现象／错误 | 处理 |
| --- | --- |
| token_required／authentication_failed | 走账号登录入口；不让用户把token贴到会话 |
| identity_service_unavailable | 保留状态，稍后重试；不能据此清除登录 |
| SKILL_AUDIENCE_DENIED／FORBIDDEN | 核对当前账号、组织和分发范围；不能换代理绕过 |
| SKILL_ARTIFACT_NOT_READY | 查看制品状态，等待构建或由维护者处理失败任务 |
| SKILL_CLIENT_INCOMPATIBLE | 升级运行宿主；保留原安装 |
| SKILL_DEPENDENCY_* | 展示具体 slug@version，解决依赖后取新计划 |
| 摘要／字节数不一致 | 保留旧版，停止激活并记录脱敏错误上下文 |
| 已安装但找不到工具 | 核对 Agent、项目、开关、权限、配置、账号以及是否需要重连 |

交付说明给出实际 id／slug／版本、目标 Agent／项目、安装和准备状态，以及未完成的业务验证。不要把短时下载地址作为永久安装链接。

来源：Fusion httpserver/skill/{skill,ticket,download,recovery}.go、application/skill/{ticket,dependencies,recovery}.go、domain/skill/{artifact,model}.go；宿主 commands/skill_market/{install,install_plan,download_ticket}.rs 及 builtin_mcp/plugin_control.rs。
