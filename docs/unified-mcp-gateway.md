# 统一 MCP 网关

内置网关可用时，平台只给 Agent 注入本地主进程的 HTTP MCP。远程业务目录由宿主
后台预热并按当前账户复用连接，不再在创建 Agent 会话时执行额外远程探测和第二次
MCP 注入。不支持宿主网关的 Agent 保留既有原生远程入口；用户自行配置的连接器不变。

## Agent 发现与使用

主进程启动时在后台读取远端 `tools/list`，此后每 **15 分钟**主动刷新。业务简介直接
来自远端搜索工具的 description，工具名称、参数和分类不在客户端维护固定清单。
缓存仅保存在当前账户内存中，不落盘；创建会话和发送回合会唤醒刷新任务，新账户按需
立即获取，未到刷新周期的成功缓存不重复请求。冷启动不等待远端 HTTP，明确报告
pending；同账户失败或过期为 stale，未登录为 signed_out，不能据此声称没有能力。

本地 tools/list 将最新简介附在统一搜索工具上，真实用户回合也通过已有私有上下文
携带当前快照。刷新后向已连接客户端发送 tools/list_changed；不支持工具热更新的
适配器仍能从下一回合快照获得最新目录信息。

远端与 search/read/invoke 同层的其他顶层工具动态保留为直接工具，完整复制描述、
inputSchema、outputSchema 和 annotations 等元数据。只改调用名称，加账户和定义
相关的不透明摘要避免重名和旧 schema 静默复用。远端三件套继续映射为本地三件套，
分组中的隐藏成员不会全部展开。未能热加载直接工具的适配器可使用快照明确给出的
capability_id，先 read 再 invoke；不能从名称推导。任务扩展式调用仍不受支持，
普通同步调用保留结果与错误，未知执行结果不得换入口重放。

- 查：search_iyw_capabilities，source=local|remote|all，默认 all。
  local 不访问远程；limit 为每个来源的候选上限。已有完整 schema 的直接工具优先。
- 浏览：source=remote、mode=browse，无需 query。用户问“有哪些能力”时跟随
  next_cursor 直到空；只浏览某组时传其 capability_id 为 group_id，分页保持同组。
  游标按账户和分组隔离并有容量限制，不依赖语义索引；失效后重新浏览。
- 读：read_iyw_capability。远程分组保留工作流说明，items 提供成员 capability_id、
  完整 input_schema、usage、参数来源和结果解释。读过完整成员后不用再次读取。
- 执行：invoke_iyw_capability。业务参数仍放 arguments；宿主转发当前目录给出的
  tool_id 和 tool_version。分组不可执行，不从名称推导 ID。

工具描述、MCP initialize instructions、主提示词和内置 gateway Skill 均声明这些规则。
本轮仅更新应用内置 Skill，不同步独立 skill 仓库。

## 生命周期和恢复

连接和不透明 ID 映射按当前账户令牌指纹隔离；每次远程请求重新核对当前账户。
并发首次连接共用初始化锁，业务请求不持有该锁。连接失败短暂冷却，传输失败使连接
失效，后续独立请求重新建立连接；不自动重放失败的业务操作。目录映射有容量上限，
淘汰最久未使用项；被淘汰、应用重启或账户变化后的旧 ID 要重新搜索。

取消请求尽力转发到远端，不保证下游副作用被撤销。执行超时、取消或断线保持
execution_status=unknown，需要原任务状态或幂等证据再决定后续操作。
TOOL_CHANGED 且 not_started 可重读旧 capability_id，并使用新返回的成员 ID。
远程目录失败返回 remote_catalog.status=unavailable，同时保留可用的本地搜索结果。
远端顶层工具变更或下架后旧直接工具身份返回 remote_tool_changed/not_started，读取
最新快照后选择当前入口；该错误不授权重放此前结果未知的业务操作。

## 服务端配合

D:\projects\iyw-mcp-gateway 为已确认的远端源码。新版本提供明确 ID/名称搜索快路、
30 秒有界向量查询缓存、同查询并发合并，以及 read_mcp_tool.include_legacy_tools=false。
客户端只在实时 schema 宣布该参数时使用，旧版远端仍可接入。

目录调用在执行前刷新 SSE 响应头，让共享 MCP 连接可以继续调度后续请求。
建议先发布远端，再发布客户端，以免旧版远端延迟响应头导致慢调用串行等待。
数据库和 9.9 部署仅作只读核对，本轮未修改线上配置、数据库或部署。

## 验证边界

客户端遵守仓库禁止本机桌面编译/测试的约定：完成静态调用链审查、rustfmt 解析、
Cargo 依赖元数据检查和 diff 检查。真实 Agent 端到端启动及取消流程、首字延迟分位数
仍需发布环境验证，不能由移除 3 秒探测预算推导固定提速幅度。
