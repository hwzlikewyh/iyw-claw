# 内置浏览器全局 Agent 访问设计

## 背景

当前内置浏览器采用逐页签授权模型。页签可处于 `user_only`、
`private_connection`、`shared_conversation` 或 `orphaned_connection` 状态，
`browser_list_tabs` 只向调用方返回身份匹配的页签。当前 Agent 没有可见页签时，
`browser_open` 返回 `BROWSER_TAB_ACCESS_DENIED`，Agent 因而要求用户点击
“Share tab with Agent”。

产品决策已调整为：iyw-claw 管理的全部内置浏览器页签对应用内所有 Agent 会话
开放。用户不再逐页签授权，也不再保留私密页签开关。该范围不包含系统 Chrome、
Edge 或其他应用的浏览器窗口。

现有同页导航约定也需要在运行时落地。Agent 默认应复用当前活动页签，只有明确
请求新页签时才创建，避免模型遗漏 `tab_id` 后不断增加页签。

本设计取代
`2026-08-16-browser-mcp-tab-id-schema-design.md` 中关于逐页签共享、零共享页签拒绝
和省略 `tab_id` 即创建新页签的约束；该文档的 `browserTabId -> tab_id` 字段映射
仍然有效。

## 目标

1. 任意 Agent 会话都能查询并操作全部内置浏览器页签。
2. `browser_list_tabs` 返回完整页签清单和可用于默认导航的 `activeTabId`。
3. `browser_open` 默认同页导航，只有 `new_tab: true` 才明确创建新页签。
4. 空页签状态下，Agent 可直接启动内核并创建首个页签。
5. 删除逐页签授权状态、共享入口、IPC 和拒绝错误，保持前后端语义一致。
6. 保留用户接管、主动暂停 Agent、操作取消和登录 profile 持久化语义。

## 核心契约

### 全局访问范围

- 所有已连接 Agent 共享同一个 `BrowserSessionManager` 页签集合。
- Agent 可看到每个内置页签的 `browserTabId`、标题、URL、运行状态、视图状态和
  控制状态。
- Agent 可使用现有浏览器工具操作任意可用页签，不再按 connection 或
  conversation 过滤。
- 浏览器 profile、Cookie 和登录态仍由现有持久目录管理，不复制到 Agent 配置，
  也不通过 MCP 返回原始 Cookie 或密码。

### `browser_list_tabs`

`browser_list_tabs` 是只读查询，不因列表为空而启动浏览器内核。`activeTabId`
固定表示 UI 当前页或下一次默认导航目标，不再兼作本次操作结果：

1. 优先选择快照顺序中第一个可见 docked host 的活动页签。
2. 其次选择快照顺序中第一个可见 detached host 的活动页签。
3. 最后选择 `tabs` 快照顺序中的第一个页签作为确定性回退。
4. 逻辑页签集合为空时返回 `activeTabId: null` 和当前 runtime 状态。

`tabs[].browserTabId` 继续作为其他浏览器工具输入 `tab_id` 的唯一页签标识。
列表可包含非 live 页签，Agent 应根据 `status` 判断是否可立即操作。

所有 Agent 工具响应新增 `targetTabId`：`browser_list_tabs` 返回 `null`，执行页签
操作的工具返回本次实际目标。显式操作 B 页签不会擅自切换 UI 当前显示的 A 页签，
因此响应可以同时返回 `activeTabId: A` 和 `targetTabId: B`。每次响应都通过同一个
resolver 重新计算 `activeTabId`，下一次默认导航不会依赖上次响应的目标字段。

### `browser_open`

输入 schema 新增可选布尔字段 `new_tab`，默认值为 `false`。执行顺序固定为：

1. 同时传入 `tab_id` 和 `new_tab: true` 时返回
   `BROWSER_INVALID_ARGUMENT`，不产生副作用。
2. 传入 `tab_id` 时导航该页签。
3. 未传 `tab_id` 且 `new_tab` 为 `false` 时，导航当前 `activeTabId`。
4. `new_tab: true` 时创建新页签。
5. 只有逻辑页签集合确实为空时，默认导航才创建首个页签。

非空集合中即使所有页签均处于 creating、navigating、crashed 或 gone，也不得自动
创建新页签。默认路径选择 `activeTabId` 后进入现有导航状态机，由其返回 busy、
crashed 或 gone 错误。这样“默认不增加页签”和“仅空浏览器创建”在异常状态下也
成立。

创建页签继续复用 `create_browser_tab_with_id()`，由其现有
`ensure_runtime_running()` 路径完成 capability 校验、内核启动和页签绑定。若存在
可见 docked host，新页签绑定到该 host；否则页签保持 unclaimed，用户以后打开
浏览器面板时由现有 host reclaim 流程接管。

`browser_open` 成功后以 `targetTabId` 返回实际导航或创建的页签 ID；
`activeTabId` 仍按统一 resolver 计算。Agent 不会进入授权等待流程。

## 架构调整

### 移除逐页签权限状态

Rust 核心删除 `AgentAccess`、`TabRecord.agent_access`、
`TabRecord.access_generation` 和 `BrowserTabSnapshot.agent_access`。页签预留、创建、
恢复及 CDP popup 继承路径不再接收或比较访问范围。

`snapshot_for_agent()` 的身份过滤被删除；Agent 状态直接基于全局浏览器快照投影。
`ensure_agent_access()`、`set_tab_agent_access()` 以及相关
`BROWSER_TAB_ACCESS_DENIED` 分支一并删除。

CDP popup 仍校验 opener、target、runtime generation 和页签生命周期，但不再校验
已经不存在的 access generation。这样 popup 与其父页签一样天然对所有 Agent
可用，同时保留防止陈旧 target 绑定的代际校验。

`PopupSeed` 用 opener 的 `tab_generation` 取代 `access_generation`。popup commit
继续先验证 popup 自身 `TabTicket`，再验证 opener 的 tab ID、target ID、状态和
`tab_generation`。opener 已变化时返回带 opener 页签及 generation 上下文的
`BROWSER_STALE_GENERATION`，不复用已经删除的授权错误。

### 保留人机控制协调

`ControlGate` 继续按页签串行化 Agent 操作，并保留以下行为：

- 用户产生语义输入时取消当前 Agent 操作。
- 用户活动期间 Agent 排队等待，超时返回 `BROWSER_USER_ACTIVE`。
- 用户主动暂停 Agent 时返回 `BROWSER_USER_HELD`。
- 页签关闭时取消活动操作并清理等待队列。

删除 `agent_enabled` 和 `reset_agent_access()`。控制门只处理并发和用户接管，不再
承担访问授权职责。不同 Agent 同时操作同一页签时继续进入现有 FIFO 队列。

### 启动和首个页签并发协调

manager 新增两个进程内共享协调锁：

- runtime start 锁覆盖“重新读取 runtime 状态、决定是否启动、等待启动完成”。所有
  `ensure_runtime_running()` 调用者共用该锁；并发调用等待首次启动结果后重新读取
  状态，不再因观察到 `starting` 立即返回 unavailable。
- tab open 锁覆盖普通页签创建，以及 Agent 默认打开时的“重新读取全局页签集合、
  选择现有目标或创建首个页签”决策。锁内必须再次读取状态，不能使用加锁前快照。

锁顺序固定为 tab open -> runtime start -> 短生命周期 state lock。不得持有 state
读写锁等待 sidecar 或页面操作；导航在选定目标并释放 tab open 锁后进入现有
`ControlGate`。

浏览器面板的自动首页不能继续使用无条件 `browser_create_tab`。新增内部桌面命令
`browser_ensure_initial_tab`，在 tab open 锁内仅当全局逻辑页签集合为空时创建；
否则只返回当前快照。工具栏“新建页签”和 MCP `new_tab: true` 仍走明确创建入口，
每次调用各创建一个页签。

### 前端和 IPC

- 删除 `AgentAccess` TypeScript 类型及 `BrowserTabSnapshot.agentAccess`。
- `browser_create_tab` 不再接收 `access` 参数；用户新建页签天然全局开放。
- 新增 `browser_ensure_initial_tab`，只供 docked 浏览器空状态自动首页使用。
- 删除 `browser_set_tab_agent_access` Tauri command、前端 API 封装和 handler 注册。
- 删除浏览器工具栏的共享/私有按钮及对应国际化文案。
- 保留“暂停/恢复 Agent 控制”按钮，因为它属于临时控制协调而非访问权限。

前端不新增授权 toast、确认对话框或 pending request。此前讨论的“一次授权后原
`browser_open` 自动续跑”方案被本设计取代。

### MCP 工具说明

- `browser_list_tabs` 说明改为查询全部内置页签，不再要求用户共享。
- `browser_open` 说明默认使用 `activeTabId` 同页导航；只有用户明确要求新页签时
  传 `new_tab: true`。
- 所有工具响应区分 `activeTabId` 与 `targetTabId`，避免把操作目标误当成 UI
  当前页。
- 其他浏览器工具继续要求使用 `tabs[].browserTabId` 作为 `tab_id`。
- schema 中删除所有“Share tab with Agent”和零共享页签停止操作的指引。

## 数据流

### 查询与默认导航

1. Agent 调用 `browser_list_tabs`。
2. delegation listener 继续验证 MCP token 并解析当前连接，仅用于调用归属、日志和
   取消，不用于过滤页签。
3. manager 获取全局快照并计算 `activeTabId`；`targetTabId` 为 `null`。
4. Agent 将返回的 `browserTabId` 用于精确操作；若只需当前页导航，可直接调用
   `browser_open` 而不传 `tab_id`。

### 空浏览器首次打开

1. Agent 调用 `browser_open({"url": "https://example.com"})`。
2. manager 获取 tab open 锁并在锁内确认逻辑页签集合为空。
3. 创建路径通过 runtime start 锁启动或复用同一个内核；并发调用等待后重新判定。
4. 创建成功后返回完整页签清单，并以 `targetTabId` 标识新页签。
5. 用户随后打开浏览器面板时，现有 docked host reclaim 未绑定页签并开始帧流。

若 Agent 首次打开与 docked 面板自动首页并发，二者都在 tab open 锁内重新判断。
先完成的一方创建首个页签，后完成的一方复用现有集合，不再额外创建首页。

## 错误与取消

- `tab_id` 与 `new_tab: true` 冲突，或 `new_tab` 不是布尔值：返回
  `BROWSER_INVALID_ARGUMENT`，不执行导航或创建。实现必须严格解析布尔值，不得把
  非布尔值静默降级为 `false`。
- URL 解析失败、协议不允许或 URL 内含用户名/密码：保持现有
  `BROWSER_NAVIGATION_FAILED` 契约。
- 显式页签已关闭或不存在：返回现有 `BROWSER_TAB_NOT_FOUND`、
  `BROWSER_TAB_GONE` 或 `BROWSER_TAB_CRASHED`；Agent 应重新查询页签。
- 内核缺失、校验失败或启动失败：保留现有 runtime/capability 错误，不伪装成
  权限问题。
- 用户活动或主动暂停：保留 `BROWSER_USER_ACTIVE` 和 `BROWSER_USER_HELD`。
- Agent 请求在创建后取消：继续关闭本次新建页签；清理失败时设置
  `effectMayHaveOccurred`。
- Agent 请求在导航期间取消：沿用现有 control epoch 与
  `effectMayHaveOccurred` 语义，不自动重试可能已发生的导航。
- server runtime 继续返回 `BROWSER_UNSUPPORTED_RUNTIME`，本次不引入无界面浏览器。

`BROWSER_TAB_ACCESS_DENIED` 从浏览器错误枚举和运行路径删除。交付前必须搜索确认
代码、MCP schema 和浏览器国际化文案中不存在残留授权提示。

## 日志与安全

- 保留工具名、connection ID、conversation ID、目标页签 ID、结果和错误码日志。
- 内核启动、默认页签选择、新建或复用分支写入结构化日志，便于区分导航行为。
- 不记录 token、Cookie、密码、表单内容或完整页面快照。
- 全局开放意味着任意已启用 Agent 都可操作同一持久 profile 中的登录页面；这是
  本设计的明确产品边界，不再提供逐页签隔离承诺。

## 兼容性

- `browser_open.new_tab` 是 MCP 输入 schema 的新增可选字段；旧调用不传该字段时
  自动采用新的同页导航默认值。
- Agent 工具输出新增 `targetTabId`，并把 `activeTabId` 统一为 UI/默认目标语义。
- `tabs[].browserTabId -> tab_id` 的字段映射保持不变，其他浏览器工具无需改参数。
- MCP 页签输出不再包含 `agentAccess`；该字段此前只描述宿主内部授权状态，不是
  Agent 选择页签所需的稳定标识。
- `browser_create_tab.access` 和 `browser_set_tab_agent_access` 是桌面前后端内部 IPC，
  前后端在同一版本中同步删除，不提供混合版本兼容层。
- 浏览器状态不跨应用进程持久化，因此升级重启时不存在旧 `AgentAccess` 状态迁移。
- profile 路径和磁盘数据格式不变，已有 Cookie、缓存、历史和登录状态继续复用。

## 验证

遵循仓库约定，不新增或运行单元测试、集成测试、端到端测试或桌面构建。交付前
执行：

- Rust 格式检查和 TypeScript/JSON 格式检查。
- 解析 `tool_schema.json` 及全部国际化 JSON。
- 定向 ESLint 或等价静态检查覆盖修改的前端文件。
- `git diff --check`。
- 沿 `browser_list_tabs -> snapshot -> activeTabId` 静态核对全部页签可见。
- 沿 `browser_open -> active tab/new tab -> runtime -> tab action` 静态核对五种输入
  分支、取消和创建后清理。
- 静态核对 runtime start 与 tab open 锁的唯一顺序、锁内状态复查，以及 docked
  自动首页与 Agent 首次打开的竞争路径。
- 沿 MCP token -> listener -> manager -> control gate 静态核对身份仍用于归属和取消，
  但不再用于授权过滤。
- 沿用户输入、暂停、页签关闭和 popup 路径核对资源释放及代际校验仍闭环。
- 搜索确认不存在 `AgentAccess`、`agentAccess`、
  `browser_set_tab_agent_access`、`BROWSER_TAB_ACCESS_DENIED` 和共享页签提示残留。

## 验收标准

1. 任意 Agent 调用 `browser_list_tabs` 都能获得全部内置页签及活动页签 ID。
2. 逻辑页签集合为空时，`browser_list_tabs` 正常返回空列表，`browser_open` 可直接
   创建首个页签；集合非空但页签忙碌或崩溃时返回现有错误且不新增页签。
3. `browser_open` 默认不增加页签；`new_tab: true` 每次只新增一个页签。
4. 显式 `tab_id` 始终优先精确导航，冲突参数不会产生副作用。
5. 并发首次打开与 docked 自动首页只产生一个首个页签；并发 runtime 启动等待同一
   次启动结果。
6. 每次响应的 `activeTabId` 都表示 UI/默认目标，`targetTabId` 表示本次操作目标。
7. 界面、工具 schema 和运行时错误不再包含共享页签指引或授权拒绝。
8. 用户接管、暂停 Agent、独立窗口、关闭页签和持久登录缓存行为保持不变。

## 非目标

- 不开放系统 Chrome/Edge 中未由 iyw-claw 管理的页签。
- 不新增基于 Agent、连接或会话的浏览器隔离。
- 不返回 Cookie、密码、localStorage 原文或其他凭证数据。
- 不修改帧流协议、帧率、窗口关闭、profile 目录或缓存同步实现。
- 不包含版本升级、安装包构建或发布。
