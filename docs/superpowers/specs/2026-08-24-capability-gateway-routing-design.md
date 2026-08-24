# 能力网关统一发现与自我改进 Skill 退役设计

## 1. 背景与目标

当前 `iyw-claw` 已经通过内置 MCP 能力网关暴露 39 个宿主能力，覆盖记忆、浏览器、
会话、用户交互、任务文件、频道、媒体和自动化。网关的稳定能力 ID、读取和调用流程
已经存在，但 Agent 仍可能先查 `list_mcp_resources`，或仅凭表面工具列表判断“没有
记忆能力”。原因不是能力不存在，而是发现路径、语义索引和独立 Skill 之间存在重复
和冲突。

本设计的目标是：

1. 让所有宿主工具通过一个稳定、可解释、支持中文自然意图的网关发现层被找到。
2. 把 `self-improving` 的有效记忆规则合并进网关，彻底从 Skill 发布源、内嵌清单和
   受管 Agent 技能目录退役该 Skill。
3. 提供独立的当前用户资料读取能力，避免用记忆检索承担账户身份查询。
4. 保留包含 CLI、认证、业务参数和领域 reference 的真正工作流 Skill，不把业务实现
   复制进通用网关。
5. 在前后兼容、权限隔离和旧安装迁移方面形成可验证闭环。

## 2. 当前实现与问题证据

### 2.1 网关发现链路

- `src-tauri/src/acp/builtin_mcp/gateway.rs` 只公开
  `search_iyw_capabilities`、`read_iyw_capability`、`invoke_iyw_capability` 三个角色。
- `capability.rs` 从 `delegation/tool_schema.json` 读取工具，使用
  `capability_registry.rs` 的稳定 ID 绑定，并按会话 feature 过滤可用能力。
- `capability_metadata.rs::capability_aliases` 仅从 ID 路由段机械生成英文别名；
  `search_score` 只做 ASCII 小写后的子串和空格分词匹配。
- 公开描述会把内部工具名替换成稳定 ID。这样可以避免 Agent 猜内部名字，但也减少了
  “记住、称呼、查我是谁”等中文自然表达的命中率。

### 2.2 记忆与账户资料

- `append_user_memory`、`propose_user_memory`、`memory_recall` 已在稳定能力注册表中，
  但当前 Skill 和旧说明存在“直接写文件”与“调用宿主能力”两套相互冲突的规则。
- `memory_recall` 的说明允许主动弃权，调用方无法稳定区分“没有匹配证据”和“索引/服务
  不可用”；短查询可能在 trigram 索引层直接被拒绝。
- `commands/iyw_account.rs::iyw_account_get_profile_core` 已能读取登录用户资料，
  但该能力目前只作为前端命令/HTTP 端点存在，未纳入 Agent 网关目录。
- `IywAccountProfile` 包含 user id、电话、积分等字段，不能原样交给 Agent；网关需要
  只返回经过字段白名单处理的显示资料。

### 2.3 Skill 发布与安装

- `iyw-claw/src-tauri/experts/experts.toml` 和 `skill/experts.toml` 都声明了
  `self-improving`。
- `commands/experts.rs` 对该 Skill 有独立的 `include_dir!`、嵌入映射和安装/刷新路径。
- 启动前的 central Skill reconcile 会把清单中的系统 Skill 发布到 Agent 技能目录，因此
  仅删除源目录而不处理迁移会留下旧链接或旧副本。
- `~/.iyw-claw/self-improving/` 是 Agent 运行反思状态，不等于 Skill 发布内容；退役
  Skill 时必须保留该目录。

## 3. 方案比较

### 方案 A：只增强提示词

把记忆和其他工具的触发词写进网关 Skill 和启动提示，保持现有搜索实现。

优点是改动小、兼容性高；缺点是依赖模型遵循长提示，无法修复中文检索、错误语义和
账户资料缺少独立能力的问题。不能作为最终方案。

### 方案 B：为每个意图增加顶层专用工具

新增 `get_current_user_profile`、`remember_user_fact` 等多个顶层 MCP 工具，让 Agent
绕过搜索。

优点是直观；缺点是工具面快速膨胀，重复现有稳定能力，权限、schema 和版本维护成本
上升。只对用户资料这种与记忆本质不同的能力采用专用稳定能力，不为每个工具复制一层
顶层 facade。

### 方案 C：网关统一索引 + 少量领域能力（推荐）

保留三件套和稳定 ID，增加机器可验证的中英文意图元数据、Unicode-aware 搜索、清晰
的 memory 结果状态，以及一个脱敏的用户资料能力。把仅用于宿主工具路由的规则合并到
网关；保留有实际脚本和业务 reference 的领域 Skill。

该方案改善发现和错误解释，同时不破坏现有直接工具兼容路径，改动边界最符合当前架构。

## 4. 目标设计

### 4.1 网关能力元数据

为每个稳定能力补充显式元数据，至少包括：

- `aliases`: 中英文同义词和常见口语表达；
- `intent_terms`: 动作词、对象词和场景词；
- `negative_terms`: 明确排除的相邻能力；
- `category`、`required_inputs`、`sensitive_fields` 和当前可用状态；
- 一条短的 `when_to_use` 说明，直接告诉 Agent 何时搜索该能力。

元数据覆盖率在启动加载时校验，缺失条目导致 catalog 校验失败，而不是静默退回到仅
按 ID 搜索。搜索结果继续返回稳定 `capability_id`、schema digest 和 catalog digest，
新增字段保持可选以兼容旧客户端。

### 4.2 搜索与路由

#### 4.2.1 Agent 预置提示词门禁

`src-tauri/src/acp/builtin_agent_prompt.rs` 的预置提示词在能力路由规则之前增加一条
明确前置要求：Agent 启动后，凡是当前目标可能需要 iyw-claw 宿主状态或动作，必须先
读取当前安装的 `iyw-capability-gateway` Skill，再检查实际 callable surface，确认三件
套是否完整，最后才执行 search -> read -> invoke。这里的“读取”走 Agent 当前支持的
Skill 加载路径，不是猜测一个 MCP 工具名，也不是调用 `list_mcp_resources`。

如果该 Skill 不可读，提示词必须要求 Agent 只依据实际可见工具继续，并把网关不可用作为
具体限制；不得伪造 Skill 内容、网关命名空间或内部能力 ID。该门禁只改变发现顺序，不
绕过当前会话的 feature、权限和用户确认策略。

搜索流程保持“搜索 -> 读取 -> 按当前 schema 调用”，但评分改为：

1. Unicode 小写/规范化，不再只依赖 ASCII；
2. 中文连续词、英文词和稳定 ID 分段分别匹配；
3. 精确能力 ID、动作+对象组合、显式 alias、描述文本按不同权重排序；
4. 查询为空、过长、无命中仍返回明确的参数错误或空结果，不猜内部工具名；
5. 同一目标只允许一次近义词重试，继续沿用当前网关的调用失败即停止规则。

读取门禁之后，网关内置提示只保留一份短路由表，至少覆盖：

| 用户意图 | 首选路由 |
| --- | --- |
| 查当前登录用户姓名、昵称、称呼 | 用户资料能力 |
| 记住用户明确要求长期保留的事实/偏好 | confirmed append |
| 可能长期有用但尚未确认的纠正/偏好/事实 | candidate propose |
| 查历史记忆 | memory recall |
| 用户反馈、追问、会话状态、任务文件、频道、浏览器、自动化 | 对应网关能力 |

这张表只负责发现，不复制领域 Skill 的执行细节。

### 4.3 用户资料能力

在现有 `iyw_account_get_profile_core` 之上增加一个网关可调用的只读稳定能力，建议
ID 为 `iyw.session.user_profile.read.v1`。能力不接收 token、用户 ID 或查询词，直接
读取当前会话登录资料。

返回字段采用白名单：`logged_in`、`display_name`、`preferred_name`、`organization_name`。
禁止返回 user id、电话、积分、头像地址、refresh token 或任何账户凭证。未登录返回
明确的 `logged_out` 状态；远端刷新失败返回 `profile_unavailable` 和脱敏错误类别，
不得伪造姓名，也不得回退到 memory recall。

### 4.4 记忆契约

将 `self-improving` 的有效内容压缩为网关内的五层规则：M0 当前会话、M1 candidate、
M2 confirmed、M3 profile、M4 interaction principles。写入只允许宿主能力调用，删除
文件直写示例和 shell fallback，保留禁止存储 secrets、推断敏感属性、第三方资料和
一次性任务状态的边界。

召回结果区分：

- `matched`: 有证据的记忆；
- `no_evidence`: 索引可用但没有匹配，不代表事实为假；
- `unavailable`: 存储、权限或索引不可用；
- `invalid_query`: 参数不合法。

对一到两个字符的查询使用规范化精确/受限回退路径；trigram 只作为长文本检索路径，
避免短词被直接拒绝，同时保持查询长度和结果数量上限。

### 4.5 Skill 分层与退役

按以下规则处理现有 Skill：

- 仅承担宿主工具发现、记忆路由或网关调用说明的 Skill：合并到网关后退役。
- 包含 CLI、认证、业务域参数和 reference 文件的 Skill：继续保留，只保留领域路由
  描述，不复制到网关。
- 计划、技能创建/安装等 Agent 工作流 Skill：继续保留，它们不是宿主工具 facade。

`self-improving` 的退役必须同时修改 `skill/experts.toml`、`skill/self-improving/`、
`iyw-claw/src-tauri/experts/experts.toml`、`commands/experts.rs` 的嵌入/映射，并增加
受管迁移 tombstone：只删除由 iyw-claw 管理的旧 central 副本、链接和受管 copy；发现
用户自有或市场覆盖内容时不做静默破坏性删除。已存在的
`~/.iyw-claw/self-improving/` 运行状态保留。

## 5. 兼容性与风险控制

- 现有三件套名称、稳定能力 ID、读取 schema 和 `delivery_ack` 规则不变。
- 直接工具仍可在实际 callable surface 显式可见时使用；网关只改变默认发现路径，不能
  伪造或重建不可见工具。
- 新用户资料能力只读且字段最小化；不会把账户凭证或内部 ID 暴露给 Agent。
- 旧版本 central Skill 清理由显式 retired-id 列表控制，禁止对整个技能根目录递归删除。
- 领域 Skill 不改认证和脚本入口，避免网关改造影响图片、企业微信、CRM 和销售工作流。
- catalog metadata、memory envelope 和 profile schema 均有版本/摘要，发生 schema 变化
  时要求 Agent 重新读取，避免复用旧参数。

## 6. 验证矩阵

实现后至少验证：

1. 39 个既有能力及新增 profile 能力都有稳定 ID、元数据和 schema 覆盖；中文/英文
   动作+对象查询能命中正确候选，近邻能力排序不倒置。
2. 每个 Agent 启动预置提示词都明确要求先读取 `iyw-capability-gateway` Skill；Skill
   不可读时能按实际可见工具继续，不会猜测网关工具或内部能力 ID。
3. gateway search/read/invoke 的现有权限、不可用状态、未知 ID、schema digest 和
   delivery acknowledgement 行为保持不变。
4. profile 只返回白名单字段，登录、退出、远端失败和空资料状态均可区分。
5. memory 的长查询、短查询、无证据、索引不可用、权限关闭和写入边界均有聚焦回归
   验证；不再出现“无证据被解释为不存在”。
6. 新版本的两个 `experts.toml` 和嵌入映射不再包含 `self-improving`；旧受管链接/副本
   被清理，运行反思目录保留，市场覆盖和用户自有文件不被删除。
7. 领域 Skill 的清单、认证前置检查和现有 CLI 路由未被改变。
8. `git diff --check`、Rust/JSON/TOML 静态检查和相关单元测试通过。遵循 iyw-claw
   规则，本机不运行桌面构建；最终报告区分静态、测试和未执行的构建验证。

## 7. 实施顺序

1. 先落地预置提示词读取门禁、catalog 元数据/搜索兼容改造和 profile 只读能力，保留旧 Skill。
2. 加入 memory 结果状态和短查询回退，并更新网关内置说明。
3. 增加 retired-id 迁移，再从两个 Skill 发布/内嵌源移除 `self-improving`。
4. 运行前后对比矩阵，确认旧直接工具、领域 Skill 和升级迁移没有回归。
