# 技能、专家包、连接器与插件：入口与分发范围

本组资料覆盖爱原物／原助理市场，核对日期为 2026-11-11。依据当前 Fusion 路由、DTO、受众策略和 iyw-claw 实现整理；表示源码支持，不表示这些功能已部署或本次已发布、下载、授权。只读当前任务所需专题。

## 按目标选择

| 用户目标 | 使用入口 | 详细说明 |
| --- | --- | --- |
| 创建一个独立技能 | 当前启用的 skill-creator；上传根目录含 SKILL.md | [上传](market-upload.md) |
| 技能组合／专家包 | 同一市场，依赖固定 slug + version | [上传](market-upload.md)、[安装](market-download.md) |
| 发布包含技能、MCP、运行时或交互页面的插件 | plugin-creator 制作合法包，再使用插件市场 | [插件包](plugin-packaging.md) |
| 发到公开市场／本组织／仅自己 | 明确 audience；创建或修改市场元数据 | 本页分发表 |
| 下载 ZIP、做安装器、更新检测 | Fusion install-plan → artifact-ticket → 对象存储 | [下载接入](market-download.md) |
| 搜索／安装／启停当前主机连接器 | 直接用 manage_iyw_connectors 的当前 schema | [连接器操作](connector-management.md) |
| 给连接器市场上传安装包、保存草稿、上架／下架 | 管理后台 connector-market；不等同于发布插件 | [连接器发布](connector-publishing.md) |
| 接入桌面／服务器的现有管理界面 | 现有 transport.invoke；不要另写安装目录 | [宿主接口](market-host-integration.md) |
| 连接器客户端、安装任务取消、OAuth／CLI授权和检查 | 现有宿主MCP管理接口 | [连接器宿主接口](connector-host-integration.md) |
| 上传企业文档并让对话引用 | 知识库上传、解析、选择与检索 | [知识库工作流](knowledge-workflows.md) |

## 对象与入口区别

- **技能 Skill**：指令和资源，入口为根 SKILL.md。脚本被打包不代表安装时会执行。
- **专家包 Expert**：根 SKILL.md 加精确版本依赖。不能用版本范围或临时“最新”替代锁定版本。
- **插件 Plugin**：入口为 .iyw-plugin.json，可声明技能、连接器、运行时、能力和应用；具体组成受 schemaVersion 约束。安装、启用、权限、配置、业务登录是不同状态。
- **连接器市场条目**：描述某个 MCP 的一种或多种接入方式，可远程连接或下载本地制品。一个条目不自动成为可分发插件。

## 发布到哪里

受众 audience 是实际访问判定；visibility 是旧展示／管理维度；publisherType 是发布者身份。三者不能互相代替。

| 目标 | 显式 audience | 建议配对 visibility | 实际可读者 |
| --- | --- | --- | --- |
| 个人私有 | owner_private | private | 创建者；按当前登录身份核对所有权 |
| 组织共享 | organization | public | 与发布目标 audienceCaihongId 相同的已登录成员 |
| 公开市场 | global_market | public | 已登录的市场用户；公开不等于匿名下载 |

普通用户创建时，组织目标来自身份服务返回的 caihongId；用户 API 不接受客户端指定另一组织。不传 audience 时，普通用户 public 默认 **organization**，private 默认 owner_private；官方 public 默认 global_market。不能把 public 直接翻译为“全平台公开”。

普通用户的技能、专家包和 v1/v2 插件均可显式选择 global_market，不需要管理员会话。文件先逐个 PUT TOS，再调用用户 complete；v2 仍须满足完整清单、准备声明和最低宿主版本校验。

publisherType=user／official 与 audience 独立，official 标识只能由管理员维护。组织成员可读不等于成员都能修改；客户端以响应 ownedByMe、canManage 判断操作入口，服务端最终授权。用户 v2 组织分发与其他用户包一致，目标来自当前账户的 caihongId，再验证目标成员访问。

owner_private 按实际创建者判断：用户上传的 v2 私有包属于该用户；官方包仍属于平台身份。该字段不能指定任意其他个人，也不代表转移所有权。

### 已有项目切换范围

下列是 Fusion 用户 API 请求体示意；id 必须换成实际返回值。每次只提交用户要求的范围，不三次都执行：

```json
{"id":"<实际市场ID>","visibility":"private","audience":"owner_private"}
```

```json
{"id":"<实际市场ID>","visibility":"public","audience":"organization"}
```

```json
{"id":"<实际市场ID>","visibility":"public","audience":"global_market"}
```

以上发往 POST /skills/update；返回 data.skill，核对其 audience、visibility 和访问状态。普通用户路由不接受 audienceCaihongId；管理员更新接口可显式指定合法组织目标。

改为公开市场前，根包和整个依赖图必须都能公开分发；不能让公开包依赖组织／个人私有包。更改受众不修改已发布版本的字节、清单和摘要。

主机 manage_iyw_connectors.publish_plugin 的 request 支持可选 audience；公开市场直接传 global_market，组织传 organization，个人传 owner_private，避免先发错受众再改。旧宿主 schema 尚未提供该字段时，以该宿主真实 schema 为准，创建后按用户授权用 update_market_metadata 设置目标受众。仅生成包的请求不包含外部发布授权。

“个人市场”还可能指某个原生 Agent 的本地 marketplace.json。它与本平台 owner_private 云项目是不同存储和分发机制，不能用生成本地索引代替云端上传成功。

## 地址与认证

| 服务 | 基础地址 | 鉴权及返回 |
| --- | --- | --- |
| 技能／专家包／插件用户 API | https://gateway.iyw.cn/iyw-fusion-api | 登录 token；通常 code=1，data 为结果 |
| 连接器用户发布 API | 同一 Fusion 基址 /connector-market/* | 登录 token；先 TOS 直传，再提交 uploadToken 和声明 |
| 连接器用户目录 | https://gateway.iyw.cn/iyw-fusion-mcp-gateway/api/connector-market | 登录 token；宿主解析 data |
| 管理后台 | Fusion 基址下 /admin/api | admin-token；普通 Agent 通过后台操作，不索要管理员凭据 |
| 当前桌面／服务端 | 已配置 transport 的 /api 入口 | 使用当前宿主认证，不能使用云端 token 替代宿主权限 |

Agent 能使用现有直接工具时优先用直接工具；JSON 查询和元数据操作可按 [HTTP约定](iyw-http.md) 使用 fetch_iyw_url。该工具由主机注入允许域登录态，参数不要加入 token、tokenInfo、Cookie、admin-token。

原始客户端集成从自身会话取得 token；对象存储的短时 URL 只用签发方要求的 headers，不转发平台凭据。multipart、二进制直传／下载应由现有上传器或获授权的客户端承担，不能把 fetch_iyw_url 的 JSON 响应当 ZIP。

Fusion JSON常见失败包为code=2、data.errorCode和message；HTTP200也可能失败。连接器目录及宿主响应各按本身契约解析，不能共用成功码推断。

## 管理后台技能接口全表

前缀 /admin/api，必须是已有管理员授权上下文；本表解释管理能力，不授予代理管理员身份。JSON 用 POST，只有列表／分类为 GET；兼容上传使用 multipart。

| 接口 | 入参／用途 |
| --- | --- |
| GET /skills | page、pageSize（默认50）、visibility、publisherType、status、category、q、audience、packageType |
| GET /skills/categories | 返回 data.items 分类 |
| POST /skills/detail | id、可选 version；返回 data.skill（含管理视图组织信息） |
| POST /skills/create | multipart 元数据＋manifest＋逐文件 part；创建官方包 |
| POST /skills/update | id；可选 displayName、summary、category、iconUrl、tags、visibility、audience、audienceCaihongId |
| POST /skills/delete | id；市场删除，不能代替客户端卸载 |
| POST /skills/versions | multipart：id、version、changelog、packageType、dependencies、manifest、文件 parts |
| POST /skills/versions/list | id；data.items |
| POST /skills/files | id、可选 version；文件树 |
| POST /skills/download | id、可选 version；兼容 ZIP 流，先辨别 JSON 错误 |
| POST /skills/set-official | id、official:boolean；仅修改发布者标识 |
| POST /skills/set-disabled | id、disabled:boolean；停用／恢复市场项目 |
| POST /skills/uploads/init | 与用户逐文件初始化字段一致；创建官方上传或为指定 skillId 上传；管理创建强制public＋global_market |
| POST /skills/uploads/url | uploadId、fileId；取得短时 PUT 地址 |
| POST /skills/uploads/status | uploadId；恢复上传所需状态 |
| POST /skills/uploads/complete | uploadId；验证源文件并提交版本 |
| POST /skills/uploads/abort | uploadId；中止本次上传 |
| POST /skills/versions/update-deps | id、version、dependencies:[{slug,version}]；管理端修正依赖，影响后续安装计划 |

iconUrl:null 可清空图标；管理员 visibility／audience 不接受 null。上述 update-deps 并非普通用户可修改已发布内容的入口，操作前明确影响范围。

管理uploads/init复用用户DTO，没有audienceCaihongId入参；官方上传初始化强制公开市场，不能靠同次init把官方包发到指定组织。需要变更受众时走已授权的管理update，发布流程应事先考虑可见窗口。

## 完成判定

上传要核对版本提交状态；发布要回读受众并用目标身份验证可见性；下载要校验字节与摘要；安装要验证目标 Agent 能发现技能／能力；业务账号则需独立验证。不得用上传 HTTP 200、目录中出现条目或 MCP 握手代替完整结果。

来源定位：Fusion internal/adapter/httpserver/{skill,skilladmin}/、internal/domain/skill/{audience,model,validation}.go、internal/application/skill/{write,dependencies}.go；客户端 builtin_mcp/gateway_tools.rs、connector_management/publish.rs、commands/skill_market/。
