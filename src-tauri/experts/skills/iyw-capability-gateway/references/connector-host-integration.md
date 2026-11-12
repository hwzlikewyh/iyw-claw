# 连接器宿主接口：安装任务、会话配置、OAuth与CLI

用于现有设置界面／客户端开发；Agent优先用[直接工具](connector-management.md)。服务器路径POST /api/<命令名>，桌面沿transport同名invoke。以下为请求JSON；返回直接JSON或AppCommandError，使用当前宿主认证。

## 浏览、配置和选择

| 命令名 | 请求 | 行为 |
| --- | --- | --- |
| mcp_scan_local | 无参数 | LocalMcpServer[]，真实本地ID、配置和归属 |
| mcp_list_marketplaces | 无参数 | provider数组；当前支持iyw_gateway |
| mcp_search_marketplace | {providerId,query?,limit?,offset?} | 市场条目，limit默认20且≤50 |
| mcp_get_marketplace_server_detail | {providerId,serverId} | 平台过滤后的接入方式 |
| mcp_install_from_marketplace | {providerId,serverId,optionId?,protocol?,parameterValues?,specOverride?} | 兼容同步安装，返回LocalMcpServer |
| mcp_upsert_local_server | {serverId,spec} | 宿主保存合法本地配置 |
| mcp_set_server_enabled | {serverId,enabled} | 宿主管理的全局开关 |
| mcp_set_server_apps | {serverId,apps:[<当前McpAppType枚举>]} | 控制哪些Agent配置；不等于当前对话选择 |
| mcp_remove_server | {serverId} | 移除本地连接器，遵守插件归属 |
| mcp_conversation_get | {conversationId?,agentType?} | revision、selected、connectors、reconnect_required、migrated |
| mcp_conversation_set | {input:{conversationId,revision,selected:[<本地ID>]}} | 更新后ConversationConnectors |

有conversationId时按对话取Agent；没有时必须提供有效agentType。selected最多64项，revision来自最近读取，冲突先重读。部分返回字段仍为snake_case，如reconnect_required、requires_migration；请求多为camelCase，分别按DTO解析。

specOverride是旧可选结构，不用于绕过已声明参数和可信制品校验。新界面使用接入方式＋参数和安装任务。秘密仅通过受保护表单保存，不经Agent参数。

## 可取消安装任务

| 命令名 | 请求 | 返回 |
| --- | --- | --- |
| mcp_market_install_begin | {input:{providerId,serverId,optionId?,protocol?,parameterValues?}} | InstallTask |
| mcp_market_install_list | 无参数 | 安装任务数组 |
| mcp_market_install_status | {taskId} | 指定InstallTask |
| mcp_market_install_cancel | {taskId} | 取消请求后的InstallTask |

InstallTask含id、serverId、optionId、phase、receivedBytes、totalBytes、cancellable、error、createdAt。id是任务UUID，不是连接器ID；begin后保存id，刷新页面先list恢复，不重新begin。

completed／failed／cancelled／interrupted为终态，其余按phase展示进度。cancellable=true才提供取消；取消请求后继续核对原任务，不把请求成功当作业务已撤销。

最多8个活跃任务；参数最多64项且编码总量≤1MiB。completed后重新scan，用真实本地ID继续配置和授权；任务创建成功、下载100%不等于连接成功。不自行删缓存或插件共享目录。

## MCP OAuth

| 命令名 | 请求 | 返回 |
| --- | --- | --- |
| mcp_oauth_begin | {serverId} | OAuthStatus |
| mcp_oauth_status | {attemptId} | 当前尝试状态 |
| mcp_oauth_cancel | {attemptId} | 已取消尝试状态 |

OAuthStatus为{id,state,authorization_url,error}，authorization_url为snake_case。attemptId取begin返回id；有效期5分钟，最多8个并行尝试。

当前OAuth实现需要**桌面内置星河**；不满足时返回配置错误，不能承诺server模式或任意外部Agent可完成同样流程。用户在设置页显式发起并打开验证后的地址；轮询只用attemptId，Agent不读取access／refresh token。

status成功后再connector_check；超时／取消清除旧链接，迟到响应不能完成新尝试。不自行补造第三方endpoint、scope或重定向地址。

## CLI登录与检查

| 命令名 | 请求 | 返回 |
| --- | --- | --- |
| mcp_connector_check | {serverId} | {state,message,tool_count} |
| mcp_cli_begin | {serverId,terminalId} | 成功null；宿主终端执行声明的login |
| mcp_cli_cancel | {terminalId} | 成功null；取消该登录流程 |
| mcp_cli_logout | {serverId} | 成功null；执行声明的logout |

terminalId必须来自当前登录流程持有的真实终端，不能填shell命令或别的会话终端。CLI声明由目录作者维护，不能把密钥拼进命令。

check的ready／authentication_required／failed／unverified按message解释；tool_count可为null。unverified不能显示已登录，CLI退出后仍需check；关闭终端不等于撤销供应商授权。

## 结果闭环

detail → 选择同一option → 安装任务 → 配置 → OAuth／CLI（若需要）→ check → 当前对话选择 → 必要时重连 → 实际业务验证。

分开记录网络、认证和业务失败，保留taskId／attemptId及脱敏错误，不输出秘密spec、完整认证响应或签名URL。

来源：web/router.rs、web/handlers/{mcp,mcp_install,mcp_auth}.rs；commands/{mcp,mcp_conversation,mcp_oauth,mcp_check,mcp_cli}.rs、commands/mcp_install/mod.rs。
