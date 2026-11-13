# 连接器用户发布与后台维护

普通用户使用当前登录态即可创建、上传和发布自己的连接器，无需管理员会话。用户调用 Fusion 基址 `/connector-market/*`，文件先直传 TOS，业务请求只提交任务引用、制品 ID 和声明。下面的管理接口仅供维护官方目录。

## 用户接口

JSON 请求可用 `fetch_iyw_url`，宿主注入用户 token；所有路径均为 POST，成功检查 `body.code=1`，结果在 `body.data`。

| 路径 | 请求 | 返回／作用 |
| --- | --- | --- |
| /connector-market/mine | page、pageSize、query、status | 当前用户条目、total |
| /connector-market/detail-owned | server_id | 本人的声明和 revision |
| /connector-market/uploads/init | filename、size、sha256 | uploadToken、url、method、headers、expiresAt |
| /connector-market/uploads/complete | uploadToken | 实际文件校验通过后的 id、filename、size、sha256 |
| /connector-market/uploads/abort | uploadToken | 取消原任务 |
| /connector-market/save | 下文完整声明，首次 revision=0 | 保存后的声明；server_id 自动加用户命名空间 |
| /connector-market/publish | server_id、revision | 发布已保存草稿 |
| /connector-market/unpublish | server_id、revision | 下架并保留已安装版本 |

本地制品：计算真实字节大小和小写 SHA-256 → init → 按返回 URL/headers PUT 原始字节到 TOS → complete → 将返回 id 填入接入方式 artifact_id → save → 用返回 server_id 与最新 revision publish。PUT 可用 [市场上传](market-upload.md) 中的 `market-direct-upload.mjs put`；目标 JSON 仅需 url/method/headers，不要把 uploadToken 写入报告。远程 HTTP/SSE 无本地制品时直接 save、publish。

新 server_id 填自己的短标识（1–74位字母数字开头，后续可含点、横线、下划线）；服务端按可信用户身份生成命名空间，不接受客户端指定 creator_scope。只能修改自己的条目，不能通过导入别人的完整 ID 认领旧官方条目。连接器目录本身没有插件 audience，发布进入连接器目录。

上传任务默认30分钟，以 expiresAt 为准。文件先写独立随机暂存对象；即使知道其他制品摘要也不能认领。完成会验证真实字节，再登记当前用户归属；失败保留原 uploadToken，结果未知先读取原条目或复用原 complete，禁止新建任务盲重放。取消用 abort；URL 或任务过期重新初始化。HTTP409／CONNECTOR_CONFLICT 回读最新 revision 后合并，不将 revision 清零覆盖。

完成后用 mine/detail-owned 核对 listed、published_version，再从连接器市场搜索同一 server_id；需要验证安装时按用户授权在目标宿主安装和连接检查。仅上传到 TOS 或保存草稿都不等于发布成功。

## 管理员接口

以下说明维护者如何把 MCP 接入方式发布到**连接器目录**；用户插件上传另见 [市场上传](market-upload.md)。连接器未暴露 owner_private／organization／global_market 受众字段，不能把插件市场的 audience 加进这套接口。需要个人／组织插件分发时走插件包市场。

## 地址和权限

Fusion 基址 https://gateway.iyw.cn/iyw-fusion-api；下列路径统一前缀 /admin/api/connector-market，需已有管理上下文的 admin-token。普通用户从 [连接器工具](connector-management.md) 浏览和安装，不能直接调用这些管理员路由。

Fusion 向内部连接器控制面转发，内部凭据由服务配置管理。代理不读取、索要或传输这份凭据；“控制面尚未配置／凭据未同步”交由维护者修复。

| POST 路径 | Content-Type | 入参 | 返回 |
| --- | --- | --- | --- |
| /list | application/json | page、pageSize、query、status=all／published／draft | data.items、total；条目含revision、listed、published_version |
| /save | application/json | 完整目录声明＋当前revision | 保存后的条目和新revision；不自动上架 |
| /publish | application/json | {id:server_id,revision} | 发布已保存草稿 |
| /unpublish | application/json | {id:server_id,revision} | 下架；已安装版本保留 |
| /upload | multipart/form-data | file:一个本地制品 | data.id、filename、size、sha256等；id写入artifact_id |

JSON 请求上限256 KiB；后台单个制品≤32 MiB，Fusion multipart总请求保护上限33 MiB。与技能插件的逐文件直传上限不同，不能混用。

## 声明字段

| 字段 | 含义 |
| --- | --- |
| server_id | 稳定目录ID；1–96位字母数字开头，可含点、横线、下划线 |
| name、description、owner | 名称、用途和发布者，必填 |
| category | 实际目录分类 |
| latest_version | 严格SemVer，如1.0.0；已发布版本变更要升版 |
| instructions、examples | 面向使用者的接入指导与示例；不写密钥 |
| homepage、icon_url | 正式说明页和图标 |
| revision | 服务端并发修订号，首次草稿0；更新必须用返回值 |
| listed、published_version | 发布状态；不靠客户端把listed改true来上架 |
| install_options | 1–16个接入方式；每个id唯一 |

每个 install_options 项：

| 字段 | 含义 |
| --- | --- |
| id、label、description | 稳定接入方式ID、名称与说明 |
| protocol | stdio、http或sse；与spec.type一致 |
| spec | stdio用command／args／env，远程用url及所需配置 |
| parameters | 安装参数声明，key、label、description、kind、required、secret、default_value、placeholder、enum_values |
| platforms | 支持的运行宿主OS／CPU组合 |
| agents | 实际支持的Agent枚举，不凭展示名猜 |
| protocol_versions | 兼容MCP协议版本数组 |
| auth_mode | stdio支持none／env／cli；远程支持none／header／oauth |
| runtime | 本地受管运行时需求；按实际制品填写 |
| artifact_id | /upload返回的ID；远程HTTP／SSE不能关联本地制品 |
| cli_auth | 仅auth_mode=cli使用，login、status必需，logout可选 |

platforms 可使用 windows-x86_64、windows-aarch64、macos-x86_64、macos-aarch64、linux-x86_64、linux-aarch64、windows7-x86_64、windows7-i686；只声明实际验证的平台，不能因为字段可选就声称全部支持。

## 最小远程接入声明

下例是可编辑结构，必须把URL换成自己部署的真实MCP服务；示例域不会提供工具。

```json
{
  "server_id":"company-docs",
  "name":"企业文档连接器",
  "description":"从授权的企业文档服务检索资料",
  "category":"office",
  "owner":"企业维护团队",
  "latest_version":"1.0.0",
  "instructions":"安装后在连接器设置页完成认证，再用于对话。",
  "examples":["检索指定文档"],
  "homepage":"https://example.com/docs",
  "icon_url":"",
  "revision":0,
  "listed":false,
  "published_version":"",
  "install_options":[{
    "id":"remote-http",
    "label":"远程MCP",
    "protocol":"http",
    "spec":{"type":"http","url":"https://example.com/mcp"},
    "parameters":[],
    "platforms":[],
    "agents":[],
    "protocol_versions":[],
    "auth_mode":"none",
    "runtime":"",
    "artifact_id":""
  }]
}
```

不把 none 示例套到实际要求认证的服务。header／env 使用声明参数和宿主配置界面；发布包中只保留变量引用，不能写真实API key。参数类型和认证附加字段依当前后台校验及控制面契约，不猜供应商scope、token endpoint或认证头名称。

## 本地 stdio 与 CLI 登录

本地服务有两种实际来源：受控命令／运行时，或已上传并校验的制品。维护者明确制品入口、运行时、平台和认证；不要在接入指导中要求用户复制未知shell脚本。

有制品时先 /upload 取得artifact_id，再将该ID绑定到具体接入方式。CLI声明可引用宿主提供的 ${ARTIFACT_DIR}（必须有关联制品）和 ${CONNECTOR_AUTH_DIR}。其他变量必须是已声明的非秘密参数。

cli_auth.login／status／logout各为 {command,args:字符串数组}；args最多64项，命令／参数每项≤4096字符且不含NUL、换行。不能在CLI命令引用secret参数，也不能拼接用户输入执行shell。

status是登录状态证据；返回格式必须来自该CLI的正式契约，不能仅以启动进程成功为已登录。没有真实供应商契约时只交付声明草稿并注明待确认字段。

## 发布顺序

1. 在后台新建或导入JSON，读取当前条目，保留revision。导入只加载本地草稿，不等于服务端已保存。
2. 如需制品先上传，核对filename、size、sha256和artifact_id；上传成功不等于目录发布。
3. 校验接入方式、真实地址、平台、Agent、参数、认证及版本，POST /save完整声明。
4. 保存返回后使用新revision，POST /publish {id,revision}。有未保存修改必须先保存。
5. 回读后台listed与published_version，再用普通用户市场查询确认同一server_id及接入方式可见。
6. 在目标宿主安装、配置、登录和连接检查；按授权验证一个真实只读业务操作，分开记录业务写入是否验证。

更新已发布连接器时增加latest_version，保存后再发布。HTTP409表示并发修改或版本已发布；刷新当前条目合并本次修改，不能把revision归零覆盖别人。

下架用 /unpublish；它停止新安装发现，保留已安装版本。需要停止本地连接另用宿主禁用；需要撤销供应商授权走其logout／OAuth撤销流程。

## 故障与可验证结果

- 管理请求成功但用户列表无条目：检查是否只保存草稿、是否完成publish、网关转发及目录版本。
- 制品上传成功但接入失败：核对绑定的artifact_id、摘要、入口、平台、runtime；不是再次上传同名文件即可解决。
- CORS／远程协议错误：确认服务实际支持当前MCP协议和认证，不自动改成SSE试写。
- 接口超时：先回读revision／listed／published_version；避免重发publish或重复建立草稿。

来源：Fusion internal/adapter/httpserver/admin/connector_market.go、internal/adapter/connectoradmin/client.go、docs/admin/{console-connectors,connector-market-model}.js；客户端 commands/mcp/{market,market_types,market_artifact}.rs。控制面内部未在此仓库展开的校验不当作已穷尽。
