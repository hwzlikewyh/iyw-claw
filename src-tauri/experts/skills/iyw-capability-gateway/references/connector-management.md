# 连接器：发现、安装、配置、授权、启停与插件发布

直接工具为 manage_iyw_connectors，使用当前可见身份及完整内联 schema，无需先走能力三件套。下表覆盖当前11个 operation。工具未提供时再查本机目录／管理界面，不编造同名工具。客户端安装任务、OAuth／CLI调用见 [宿主对接](connector-host-integration.md)。

## 操作总表

| operation | 参数 | 行为／结果 |
| --- | --- | --- |
| installed_list | 无其他参数 | 当前主机连接器；结果 result.connectors |
| market_search | query可选、provider_id可选、limit=1..50、offset≥0 | 搜索市场；默认provider为iyw_gateway |
| market_detail | provider_id、server_id | 返回接入方式、非敏感参数声明及版本 |
| install_market | provider_id、server_id、option_id、protocol；parameter_values可选 | 按实际接入方式安装；宿主批准后执行 |
| uninstall | server_id | 卸载指定本地连接器；宿主处理批准及归属检查 |
| set_current_conversation | server_id、enabled:boolean | 修改活动对话选择；旧全局配置迁移可能触发批准 |
| set_global_enabled | server_id、enabled:boolean | 修改全局开关；影响范围大于当前会话 |
| publish_plugin | request为新插件发布结构 | 发布含连接器的合法插件包 |
| publish_plugin_version | request为已有插件新版结构 | 上传不可变的新版本 |
| update_market_metadata | request为完整元数据结构 | 修改含连接器插件的展示资料／分发范围 |
| delete_market_item | market_id | 删除含连接器的市场插件；不是本地卸载 |

只提交该 operation 允许的字段。结果通常为 {success,operation,result}，取消为 {success:false,cancelled:true,operation}；同时检查 MCP isError，不从“工具返回了JSON”推断成功。

### 搜索与详情

```json
{"operation":"installed_list"}
```

```json
{"operation":"market_search","provider_id":"iyw_gateway","query":"文档","limit":10,"offset":0}
```

```json
{"operation":"market_detail","provider_id":"<搜索返回provider_id>","server_id":"<搜索返回server_id>"}
```

安装后的本地 id 和目录 server_id 不保证相同；后续启停／卸载用 installed_list 或安装响应返回的本地 id。installed_list 中 app_managed_global_enabled 是全局开关，missing_configuration 表示缺配置，plugin_managed 表示有插件归属；不能把这些字段等价为“已登录”。

详情 install_options 中 id、protocol、label、parameters、auth_mode、downloads_artifact 都来自目录。parameters 可能有 key、required、secret、kind、enum_values、has_default；has_default=true 不代表代理能读取默认密钥。

### 安装到接通

```json
{
  "operation":"install_market",
  "provider_id":"<真实provider_id>",
  "server_id":"<真实server_id>",
  "option_id":"<详情中的接入方式id>",
  "protocol":"<同一接入方式的protocol>",
  "parameter_values":{"<已声明的非敏感参数key>":"<实际值>"}
}
```

只传明确声明的非敏感参数。密码、API key、Cookie、OAuth token、Authorization header 不能通过 parameter_values、工具对话或日志传递；缺少敏感配置时引导用户到连接器设置界面。宿主会验证接入方式并按实际批准流程下载／安装。

安装结果中 requires_oauth_in_ui=true 时去界面授权；missing_configuration=true 时先完成配置，再检查连接。stdio 在运行宿主启动本地进程；HTTP／SSE 使用实际远程 MCP 地址。服务端工作区中的本地是服务端，不能假定可访问浏览器电脑上的程序。

```json
{"operation":"set_current_conversation","server_id":"<返回的本地id>","enabled":true}
```

```json
{"operation":"set_global_enabled","server_id":"<返回的本地id>","enabled":false}
```

修改后核对 enabled、migrated、reconnect_required 或宿主响应。运行中的 Agent 可能需要重连；按返回状态说明，不能把连接器开关当撤销供应商端 OAuth。

## 认证方式

| 模式 | 使用位置 | 完成证据 |
| --- | --- | --- |
| none | 无业务认证 | MCP连接与工具发现成功；业务是否可用另验 |
| env | stdio，秘密参数由设置界面保存 | 配置完整且连接检查通过 |
| header | 远程连接 | 宿主注入已保存认证；代理不接触实际headers |
| oauth | 远程连接 | 界面授权并完成回调／连接检查 |
| cli | stdio，目录声明login/status等 | 用户显式登录后，声明的status确认成功 |

MCP OAuth 与供应商业务登录可能分开。插件还可声明自己的 status／login／logout 工具，见 [插件准备](plugin-packaging.md)。握手成功不代表业务账号已登录。

## 通过本工具发布插件

适用于 .iyw-plugin.json 中 components.connectors 非空的 v1/v2 插件。普通用户登录即可发布，宿主逐文件 PUT TOS，Fusion 只收清单和完成请求。单纯 MCP 配置、纯技能或无连接器插件按 [市场上传](market-upload.md) 或 [连接器用户发布](connector-publishing.md) 处理。先按 [插件包](plugin-packaging.md) 校验；files 每项 {path,contentBase64,size} 仅是本地宿主输入，编码必须来自实际文件。

新插件 request 字段：

```json
{
  "operation":"publish_plugin",
  "request":{
    "slug":"my-connector-plugin",
    "displayName":"业务连接器",
    "summary":"通过已声明的MCP服务处理指定业务",
    "category":"<categories返回值>",
    "tags":["业务"],
    "visibility":"private",
    "version":"1.0.0",
    "changelog":"首次发布",
    "dependencies":[],
    "files":[{"path":".iyw-plugin.json","contentBase64":"<实际文件Base64>","size":123}]
  }
}
```

这是形状示意，必须包含整个合法包的真实 files，size 也必须替换。iconUrl 可选字符串／null；audience 可选 global_market／organization／owner_private。工具内部固定 packageType=plugin，request 不接受 packageType。最多512文件；.iyw-plugin.json≤1MiB且必须声明连接器。大包使用目录脚本逐文件直传，避免整包 Base64 工具参数。

新版使用 operation=publish_plugin_version，request 为 {id,version,changelog,dependencies?,files}；id 是上次发布返回的实际市场ID。不是重新起一个slug。

若用户要求全平台公开，先获得发布返回 id，再用完整元数据设置受众：

```json
{
  "operation":"update_market_metadata",
  "request":{
    "id":"<返回的市场ID>",
    "displayName":"业务连接器",
    "summary":"<当前真实摘要>",
    "category":"<当前真实分类>",
    "tags":["业务"],
    "visibility":"public",
    "audience":"global_market"
  }
}
```

组织共享改为 audience=organization；个人私有用 private＋owner_private。此操作要求 displayName、summary、category、tags、visibility、audience 全部提供，应先读当前元数据保留用户未要求修改的字段。不要传空数组意外清空标签。

发布和元数据修改、删除均由宿主处理批准。仅写文档／生成插件包不触发这些动作。未能完成第二步时报告“已创建，尚未确认公开分发”，不能声称公开成功。

## 原始连接器目录 API

开发者使用基址 https://gateway.iyw.cn/iyw-fusion-mcp-gateway/api/connector-market；POST /list 为 {offset,limit,query}；POST /detail 为 {id:实际server_id}。返回 data；provider_id=iyw_gateway 是宿主路由选择，不放入云目录业务体。

本地制品通过该目录的 POST /artifact-ticket，由现有安装器提交 {connector:实际server_id,id:制品摘要,ticket:目录提供的票据}。响应url、size、sha256用于直下并校验；不把内部ticket展示给用户，不把平台凭据转发到对象存储。当前下载≤32MiB，ZIP展开≤128MiB且最多2048项；摘要、文件名和缓存由安装器验证。

禁止把 Fusion /skills/artifact-ticket 与连接器目录票据混用。安装票据的metadata来自真实详情，不自行构造；后台维护见 [发布专题](connector-publishing.md)。

## 故障定位

- 列表404：核对 MCP网关、控制面部署和入口转发；不是“市场暂无连接器”。
- 不兼容／找不到接入方式：重读同条目详情，检查平台、Agent、协议和版本；不修改option_id试探。
- 缺配置／未登录：到对应设置页完成；不让代理收集秘密。
- 旧全局连接器：宿主批准迁移后再用于会话，核对 migrated、reconnect_required。
- 插件托管连接器：遵守归属和依赖，优先插件准备／卸载入口，不删其共用文件。
- 写请求超时或结果未知：查 installed_list、原市场版本或元数据；不自动重放安装／发布。

来源：builtin_mcp/gateway_tools.rs、connector_management/{request,market,publish}.rs、commands/mcp/{market,market_types,market_install}.rs。
