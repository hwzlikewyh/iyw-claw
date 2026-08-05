# Skill 市场、真实安装与 Agent 按需配置设计

## 目标

本次改造交付四个可观察结果：

1. Skill 市场采用已确认的 B3 布局：三列信息卡、右侧详情检查器、信息密度高但层级清楚。
2. “安装完成”只在真实下载、校验、落盘和 Agent 发布全部成功后出现，不再由前端定时器模拟。
3. Agent 配置目录按实际安装生命周期创建。安装一个 Agent，只创建该 Agent 的 profile；从未安装的 Agent 不产生空目录。
4. 子智能体默认配置只展示当前已安装的 Agent。没有用户覆盖时继承 Agent 自身默认值，即界面展示“默认/自动模式”。

本次不删除用户机器上已经存在的目录。Agent 卸载也不自动删除可能包含账号、模型或会话信息的 profile。清理历史空目录属于独立的、需要确认的删除操作。

## 已确认的产品语义

### Agent 是动态集合

Agent 名称和数量不得写死。

- `acp_list_agents().installed_version != null` 是“已安装 Agent”的权威判断。
- 当前安装两个 Agent，市场安装目标和子智能体设置就展示两个。
- 后续安装第三个 Agent，对应配置目录才在安装流程中创建，相关界面随后自动出现第三项。
- 未安装 Agent 不展示为可选目标，也不能因为应用启动、Skill 列表刷新或市场安装而创建 profile 目录。

`enabled` 是用户是否允许使用该 Agent，`installed_version` 是本机是否已安装，两者语义不同。目录创建和市场投放以 `installed_version` 为准；子智能体可用性还需同时满足 `enabled`。

### Skill 有一份权威副本和多个发布目标

市场 Skill 的权威副本仍保存到共享目录：

```text
~/.iyw-claw/skills/<skill-slug>/
```

每个成功安装的目录必须包含：

```text
SKILL.md
.iyw-claw-market-skill.json
```

用户在安装面板选择的已安装 Agent 是发布目标。后端只向这些目标创建链接或受管副本，不再遍历全部支持 Skill 的注册表 Agent。

默认勾选当前所有“已安装且已启用、支持 Skill”的 Agent，用户可在确认安装前取消部分目标。至少保留一个目标；否则不允许提交安装。

## B3 市场界面

### 页面骨架

页面分为四层：

1. 顶部导航：市场、组织、我的、已安装、需要更新，每项展示数量。
2. 快速筛选：官方认证、兼容性、分类、依赖和安装状态。
3. 三列目录：桌面宽屏固定三列；中等宽度两列；移动端单列。
4. 右侧检查器：展示选中 Skill 的详情、版本、文件、依赖、兼容性和安装目标。

页面是工作台，不增加营销式 hero、装饰性大卡片或嵌套卡片。

### Skill 卡片

每张卡片直接展示：

- 展示名和 slug
- 发布来源及官方认证状态
- 当前兼容状态
- 安装状态：可安装、已安装、可更新
- 最新版本；可更新时同时显示当前版本
- 制品大小、直接依赖数、更新时间
- 摘要和关键标签
- 与当前状态对应的动作：查看、安装、更新或管理

重点顺序固定为“名称和状态 > 摘要 > 版本/大小/依赖/时间 > 标签”。卡片尺寸稳定，动态文本不能造成网格跳动。

### 详情检查器

检查器默认停留在概览，提供文件、版本、依赖标签页。安装前必须明确展示：

- 兼容或阻断原因
- 选定版本、下载大小和依赖闭包
- 实时检测到的已安装 Agent 数量
- 每个目标 Agent 的中文名、安装状态和默认模式说明
- “Agent 安装一个才创建一个配置目录”的目录规则

安装按钮文案为“安装并启用”。按钮上方汇总下载大小、依赖数和启用目标数。

## 真实安装流程

### 前端状态机

删除 `use-skill-market-install.ts` 中的模拟下载计时器、查询参数故障注入和假激活阶段。安装状态由真实调用驱动：

```text
idle
  -> resolving
  -> confirming
  -> installing
  -> done | failed
```

第一阶段加载详情和安装计划用于确认。用户点击“安装并启用”后，前端调用真实 `skill_market_install`，携带 Skill ID、版本和目标 Agent 集合。

当前后端命令一次完成下载、摘要校验、ZIP 校验、共享目录原子交换和 Agent 发布，因此在没有真实字节级事件前，界面展示不确定进度和当前阶段文案，不伪造百分比、下载速度或票据刷新次数。

只有 IPC/HTTP 调用成功后才执行 `applyInstalled()` 和刷新已安装 Skill 数据。失败时保留服务端错误信息，卡片状态不得被修改为已安装。

### 安装请求

安装命令请求扩展为：

```text
id: string
version: string
agentTypes: AgentType[]
```

桌面 IPC 和 Web handler 使用相同参数及核心函数。Snowflake ID 继续按字符串传递。

后端在下载前完成以下校验：

1. 目标集合非空且无重复项。
2. 每个目标 Agent 支持 Skill。
3. 每个目标 Agent 当前确实已安装并启用。
4. 安装计划、版本、依赖闭包、制品大小和摘要合法。

目标状态在确认页打开后可能变化，因此后端必须重新校验，不能只相信前端快照。

### 原子性与错误处理

安装继续使用共享目录 swap 和回滚机制：

1. 下载并验证全部依赖闭包。
2. 准备全部共享 Skill 目录交换。
3. 只向请求中的目标 Agent 发布。
4. 任一目标发布失败，回滚本次共享目录和已完成的发布。
5. 全部成功后提交交换并返回安装结果。

日志记录 Skill ID、slug、版本、依赖包数量、目标 Agent 类型、阶段、结果和完整错误原因；不得记录 token、制品内容或 Base64。

## Agent profile 生命周期

### 初始化

`create_storage_layout()` 只创建公共基础目录，例如 runtime、config 根、downloads、staging 和 trash。删除其中遍历全部注册表 Agent 并创建 profile/env 目录的行为。

新增单 Agent 的幂等目录初始化入口。只有以下流程可以调用：

- Agent 安装或激活成功
- 已安装 Agent 首次启动前的 reconciler
- 用户显式配置该 Agent 的 profile 覆盖路径

入口接收一个 `AgentType`，只创建该 Agent 的 `profile.root` 和必要 env 目录。

### Skill 发布与启动 reconcile

以下操作都必须先解析“当前已安装 Agent”再执行：

- 市场 Skill 发布
- 启动时市场 Skill reconcile
- Skill 设置页的共享 Skill reconcile
- 托管 Skill 家族同步

禁止使用 `supported_skill_agent_types()` 直接代表已安装 Agent。它只表示能力支持，不表示本机安装状态。

Agent 卸载不自动删除 profile。卸载后该 Agent 不再出现在安装目标和子智能体列表中，也不再参与新的 Skill reconcile。

## 子智能体设置

`DelegationAgentDefaultsPanel` 不再使用静态 `AGENT_TYPES`。它复用 `useAcpAgents()`，仅展示：

```text
installed_version != null && enabled
```

加载期间不回退到全部 Agent。没有可用 Agent 时展示空状态，引导用户先安装并启用 Agent。

每个 Agent 缺少 `agent_defaults` 覆盖时：

- 模式继承 Agent 默认模式，界面显示“默认（自动模式）”或该 Agent 返回的默认名称。
- 模型继承 Agent 当前默认模型。
- 推理强度等配置继承 Agent 当前默认值。
- 保存时不为继承值写入冗余覆盖字段。

用户安装、卸载、启用或停用 Agent 后，`app://acp-agents-updated` 触发现有共享 store 刷新，列表随实时状态更新。

## 数据与组件边界

- `use-skill-market.ts` 负责目录、筛选、选中项和服务端状态刷新。
- `use-skill-market-install.ts` 只负责真实安装状态机，不模拟网络和落盘。
- 市场视图拆分为工具栏、三列列表、卡片、详情检查器和安装目标列表，单文件不超过项目限制。
- `useAcpAgents()` 是前端已安装 Agent 数据源，市场与子智能体设置共享相同过滤语义。
- Rust 安装核心负责最终授权校验和文件系统原子性，前端过滤不构成安全边界。

## 验收标准

### 市场界面

- 1440px 及以上为三列卡片加右侧检查器；中等宽度两列；移动端单列。
- 卡片完整展示名称、slug、来源、状态、版本、大小、依赖、更新时间、摘要和标签。
- 长 slug、长摘要、加载态、错误态和空状态不造成重叠或布局跳动。

### Skill 安装

- 点击安装后必须发起真实 `skill_market_install` 调用。
- 成功后 `~/.iyw-claw/skills/<slug>/SKILL.md` 和 market marker 存在。
- 所选 Agent 的 Skill 目录存在链接或受管副本；未选 Agent 不产生发布项。
- 后端失败时界面显示失败且不能进入已安装状态。
- 重启应用后“已安装”状态由本地 marker 恢复，不依赖前端内存补丁。

### Agent 目录

- 初始化存储不会批量创建所有 Agent profile。
- 新安装一个 Agent 只创建该 Agent 的 profile/env 目录。
- 市场安装、Skill reconcile 和打开设置页不会为未安装 Agent 创建目录。
- Agent 卸载后保留已有 profile，但不再参与动态列表或同步。

### 子智能体

- 只显示当前已安装且已启用的 Agent。
- 新安装 Agent 后自动出现；卸载或停用后自动消失。
- 未设置覆盖时，模式、模型和推理强度均展示并使用 Agent 默认值。

## 验证方式

按照仓库限制，不在本机编译或启动桌面端。实现交付前执行：

- 前端相关文件的 ESLint/类型静态检查（不触发应用构建）
- `rustfmt --check` 和 `git diff --check`
- 沿 IPC 与 Web 两条调用链静态复核参数、错误映射、回滚和事件刷新
- 在允许运行桌面构建的 CI 或发布环境验证上述文件系统验收项

不得保留模拟安装计时器、故障注入查询参数或临时验证文件。
