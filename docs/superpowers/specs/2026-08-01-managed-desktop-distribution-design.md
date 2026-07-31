# 托管桌面分发、Skill 与运行可靠性总体设计

## 1. 文档状态

- 日期：2026-08-01
- 状态：待用户确认后实施；当前仅完成设计与任务拆分
- 仓库：`iyw-claw`、`iyw-fusion-api`、`skill`
- 执行入口：`docs/superpowers/plans/2026-08-01-managed-desktop-distribution/README.md`
- 桌面端约束：禁止在当前机器编译、打包、启动、执行 Cargo 检查或测试
- 目标：把应用、Agent、CLI、运行时和 Skill 从“随包携带或直连上游”迁移为“后端决策、TOS/CDN 分发、本地不可变库存、原子激活和可回滚”

## 2. 已核对的代码事实

### 2.1 三仓职责

- `iyw-claw` 是 Tauri 2 + Rust + Next.js 桌面/服务端应用，负责受信执行能力、本地库存、下载校验、激活、回滚、会话启动和 UI。
- `iyw-fusion-api` 是 Go + Hertz + MySQL 5.7 后端，已有 Skill 市场、App Release Center、Agent Version Center、TOS 适配器和静态管理页。
- `skill` 是系统 Skill 的发布源，`experts.toml`、Skill 目录和稳定 SemVer 标签共同定义版本。它不是客户端运行目录。

### 2.2 Skill 下载失败是确定性契约错误

当前逐文件上传链路将原始文件大小总和写入 `Version.PackageSize`：

- `iyw-fusion-api/internal/application/skill/direct_upload_init.go`
- `iyw-fusion-api/internal/domain/skill/model.go`
- `iyw-fusion-api/scripts/mysql/011_create_latest_schema_for_prod.sql`

下载时，后端才在 `internal/application/skill/download.go` 使用 Deflate 动态生成 ZIP；桌面端 `src-tauri/src/commands/skill_market/install.rs` 却用原始文件总大小限制并校验收到的 ZIP 字节数。因此 `expected_size=19644, received_size=14339` 不是网络偶发截断，而是将 `raw_size` 误当成 `artifact_size`。

同一链路还有第二层完整性错误：逐文件版本没有真实 ZIP 对象，`ObjectSHA256` 不能代表刚刚动态生成的 ZIP；客户端通过长度检查后仍会在对象摘要校验失败。因此不能把“删除长度校验”当作修复，必须先生成并冻结实际 artifact 的 size 和 SHA-256。

TOS 当前适配器提供对象上传、读取、HEAD 和签名 URL 等对象存储能力。对象存储不能把多个原始文件在线转换成一个符合本项目确定性规则的 ZIP。正确边界是后端任务构建一次 ZIP 并上传 TOS，下载流量由 TOS/CDN 承担。

### 2.3 Skill 权限维度混在一起

当前模型只有：

- `visibility = public | private`
- `publisher_type = official | user`

`public` 会跨组织进入市场查询，`private` 只允许创建者本人；尚不能表达“同一组织内所有已登录用户可见”。可见性、发布者、是否强制安装和客户端兼容范围必须拆开。

### 2.4 后端已有版本中心，不应重建

`iyw-fusion-api` 已有：

- `internal/domain/agentrelease/`
- `internal/application/agentrelease/`
- `internal/adapter/httpserver/agentplatform/`
- `internal/adapter/httpserver/agentplatformadmin/`
- `scripts/mysql/027_create_agent_version_center.sql`
- Agent、Git、Node、uv 的不可变版本、推荐版本、最低安全版本、灰度、固定、回滚和 TOS 短时票据

本次应扩展现有 Version Center，增加统一初始化计划、PC 版本约束、Agent SDK/CLI 镜像任务和完整 TOS 托管来源，不建立第二套版本系统。

### 2.5 桌面包仍携带或直连运行时

以下现有路径与目标冲突：

- `src-tauri/scripts/prepare-sidecars.mjs` 打包 uv/uvx、Node、MinGit 和 codex-acp npm prefix。
- `src-tauri/tauri.conf.json` 的 `externalBin` 和 `resources/runtime` 将运行时带入安装包。
- `src-tauri/src/commands/runtime_bootstrap.rs` 硬编码 Node/Git 版本、URL和摘要，并优先读取包内制品。
- `src-tauri/src/acp/binary_cache.rs` 仍能直连 GitHub 下载 uv，并保留包内种子逻辑。
- `src-tauri/src/acp/registry.rs` 仍包含若干外部 Agent 下载地址。
- `src-tauri/src/commands/internet_tools.rs`、`office_tools.rs` 和消息渠道 CLI 仍各自管理外部工具来源。

现有 NSIS 已把 `app` 与 `runtime/config/data/logs` 分区，更新时只替换 `app`。该结构应保留，并把所有受管内容移出 `app`。

### 2.6 系统 Skill 仍是 Git checkout 和二进制内嵌混合模式

- `src-tauri/src/system_skills/` 仍从 GitLab 仓库拉取系统 Skill。
- Git 远端凭据存在硬编码风险，必须立即轮换并从历史与代码中移除。
- dirty checkout 更新会执行破坏性重置，可能覆盖用户修改。
- `src-tauri/src/commands/experts.rs` 使用大量 `include_dir!` 把 Skill 编进桌面二进制。
- 系统 Skill、市场 Skill、中央目录、Agent 链接和 copy fallback 的所有权边界不统一。

目标是让 `skill` 只做发布源，Fusion 后台按稳定标签生成不可变制品；客户端只处理后端目录和 TOS 票据。

### 2.7 新会话已有单入口，但需要强化

`build_session_runtime_env` 和 `spawn_agent_connection` 已在 Agent spawn 前执行 storage 协调、Skill 协调、provider overlay、模型目录更新和 PATH 注入；`provider_overlay_files.rs` 已有临时文件和原子替换。应复用该入口建设 `SessionConfigReconciler`，不在各页面重复写配置。

### 2.8 多智能体与实时反馈当前默认关闭

- `DelegationConfig::default().enabled = false`
- `FeedbackSettings` 使用派生 `Default`，`enabled = false`

新安装应默认开启；旧用户已有显式值不覆盖；没有持久值的旧用户执行一次性迁移；后台保留 kill switch。

### 2.9 消息渠道存在多处确定性断链

已确认问题：

1. 新建渠道保存 `enabled=true`，但新增对话框创建后不调用 `connectChatChannel`；后台自动连接只在应用启动时执行一次。
2. 从禁用切换为启用只更新数据库，不连接；仅禁用且当前已连接时会断开。
3. `connect_chat_channel_core` 和测试路径在创建 backend 前统一读取 keyring token；企微 backend 明确不需要 channel token，因此企微会在到达 backend 前失败。
4. 编辑对话框通过 `buildChatChannelConfig` 重建整段 JSON，会丢失后端创建时写入的 `channel_workspace_root`。
5. 微信扫码确认回写仅保存 `base_url`，同样覆盖 workspace root、默认 Agent 等字段。
6. “测试连接”只验证渠道 API/凭据，不验证入站、dispatcher、工作区、Agent storage、spawn、TurnComplete 和出站回复。
7. 配置页面没有统一 readiness，无法一次看出 Agent 未安装、storage 未激活、默认 Agent 不可解析、工作区不可写或 gateway 不可用。
8. 部分出站发送结果被忽略，用户只看到“没有回复”，缺少阶段化状态和错误关联 ID。

### 2.10 用户记忆并非完整自动学习闭环

现有 `user_memory` 模块具备事务、原子替换、候选状态、去重、手动确认、会话快照和 MCP 工具授权，但仍有关键缺口：

1. 记忆只在模型主动调用 `append_user_memory` 或 `propose_user_memory` 时更新，没有 TurnComplete 后的可靠采集队列。
2. 候选需相同规范化内容多次观察才升级；模型不稳定措辞会产生多个长期 tentative 候选。
3. 维护提示在 MCP 路由失败时硬编码写入 `C:/Users/Administrator/.iyw-claw/user-memory.md`，与实际 resolved root 不一致。
4. 设置页 TypeScript snapshot 只保留基础字段，后端返回的 candidate diagnostic、candidate counts、projected capabilities、companion health、migration report 和 availability 没有完整展示。
5. 设置页载入时清除 HTML 注释和 `[Codex CLI]`，后续保存可能移除后端使用的 entry marker，破坏去重和纠正定位。
6. 恢复会话的重新注入判断偏向 confirmed append；仅 proposal 可用时也必须刷新工具指导。
7. 缺少最近成功时间、最近失败、积压量、处理延迟、最后一次工具调用和手动重建入口。

## 3. 目标架构

### 3.1 控制面与数据面

控制面由 Fusion API 负责：

- 发布和冻结制品元数据。
- 决定哪个客户端、组织、平台、架构和渠道应使用哪个版本。
- 运行持久化任务，镜像上游并构建 Skill ZIP。
- 返回目录、resolve 结果、初始化计划和短时下载票据。
- 记录管理审计、任务审计和客户端结果事件。

数据面由 TOS/CDN 负责：

- 大文件下载。
- Range、缓存、流量和可用性。
- 短时 URL，不暴露稳定 object key。

客户端负责：

- 只接受编译内 allowlist 中的组件和 recipe schema。
- 校验大小、SHA-256、签名、目标平台、归档结构和解压边界。
- 下载到 staging，验证后原子激活。
- 保留 pin、last-known-good、回滚和离线缓存。
- 报告结果，但不把本地路径、凭据或短时 URL 发回日志。

### 3.2 统一制品模型

所有可下载内容使用同一组概念，但保留不同领域表：

- `artifact_id`：Snowflake 字符串 ID。
- `component_kind`：`skill | system_skill | agent | agent_sdk | cli | node | uv | git | auxiliary_cli`。
- `version`：规范 SemVer 或经领域明确允许的上游版本。
- `target`、`arch`、`package_kind`。
- `raw_size`：源文件总字节，仅用于构建和审计。
- `artifact_size`：实际下载对象字节，是客户端 Content-Length 校验依据。
- `artifact_sha256`、独立签名、`object_key`。
- `artifact_status`：`pending | building | ready | failed | quarantined | deleted`。
- `source_origin`、`source_version`、许可证和上游摘要。
- `min_client_version`、可选 `max_client_version`、依赖约束。
- `created_at`、`verified_at`、构建任务 ID 和审计主体。

发布后字节相关字段不可修改。重建必须创建新 artifact generation，不得在同一 ID 下替换对象。

### 3.3 PC 版本与组件策略

统一 resolve 输入：

- PC `clientVersion`、release channel、installation ID。
- OS、arch、runtime、locale。
- `orgCode` 和登录用户身份。
- 本地 inventory：组件、版本、摘要、active、pin、LKG 和健康状态。

统一 resolve 输出：

- `catalogRevision`、`planId`、过期时间。
- 每个组件的 `keep | install | update | block | remove_managed` 动作。
- 选择原因：`missing | recommended | minimum_safe | client_compatibility | mandatory | repair | pinned | lkg`。
- 依赖顺序、下载票据 endpoint、预期大小/摘要/签名。
- 强制截止时间、是否允许离线继续、失败后的恢复动作。

推荐版本变化不能自动降级；安全阻断和最低安全版本可以覆盖 pin；未知组件不能由服务端引入。

### 3.4 Skill 权限矩阵

拆成四个独立维度：

- 发布者：`official | user`。
- 受众：`global_market | organization | owner_private`。
- 分发：`mandatory | optional`。
- 兼容/定向：客户端版本、渠道、OS、arch、组织和灰度。

明确语义：

- 官方市场：所有已登录用户可见。
- 组织 Skill：只有 `skill.org_code == identity.org_code` 的已登录用户可见；同一组织内所有已登录用户可见。
- 用户私有：只有创建者本人可见。
- 强制 Skill 是分发策略，不是一种可见性。
- 列表、搜索、详情、版本、文件树、依赖解析、安装计划和下载票据必须调用同一 access policy，不能各写一份条件。
- 依赖计划中任一节点不可见、未发布、不兼容或被禁用时，整个计划失败并返回脱敏原因，不泄露不可见 Skill 元数据。

### 3.5 Skill ZIP 构建

上传完成后的状态机：

```text
uploading -> verifying_files -> artifact_pending -> building
          -> ready
          -> failed/quarantined
```

构建规则：

1. HEAD 并校验所有原始文件的路径、大小和 SHA-256。
2. 按规范化相对路径排序。
3. 使用固定时间戳、权限、压缩方法和 ZIP metadata，保证同输入同摘要。
4. 写本地临时文件并计算实际 `artifact_size` 和 `artifact_sha256`。
5. 上传临时 object key，HEAD/GET 抽检后复制或提交到不可变正式 key。
6. 在数据库事务中切换 artifact 为 ready，并提升 Skill version ready。
7. 失败保留原始文件和任务诊断，不发布半成品。
8. 历史逐文件版本通过可重入回填任务构建，完成前继续显示“制品准备中”，不得走错误的动态流契约。

客户端只从 install plan 取得 artifact ID，再换取短时票据；支持 Range 和断点续传。票据过期只刷新 URL，不重建计划或改变版本。

### 3.6 持久化任务中心

MySQL 5.7 不依赖 `SKIP LOCKED`。任务表使用条件更新式租约：

- `status`、`scheduled_at`、`priority`。
- `lease_owner`、`lease_until`、`fencing_token`。
- `attempt`、`max_attempts`、`next_retry_at`。
- `dedupe_key`、`payload_json`、`checkpoint_json`。
- `last_error_code`、有上限的脱敏 `last_error_detail`。
- `created_by`、`created_at`、`started_at`、`finished_at`。

worker 抢占必须是带旧状态和过期租约条件的单条 UPDATE；所有写入用 fencing token 防止过期 worker 覆盖新 worker。支持指数退避、抖动、取消、死信、手动重跑、分页查询、指标和审计。

任务类型：Skill ZIP、历史回填、系统 Skill 镜像、Agent/SDK/CLI 镜像、Node/uv/Git 镜像、上游发现、签名/许可证校验、TOS 存在性巡检、孤儿对象清理和目录发布。

### 3.7 桌面本地目录和更新保护

建议逻辑根：

```text
<root>/app/                         # 仅应用更新替换
<root>/runtime/<tool>/<version>/    # Node/uv/Git
<root>/agents/<id>/<version>/       # Agent/SDK/CLI
<root>/skills/system/<id>/<version>/
<root>/skills/market/<id>/<version>/
<root>/skills/user/<id>/            # 用户拥有，不自动覆盖
<root>/inventory/
<root>/staging/
<root>/config/
<root>/data/
<root>/logs/
```

每个受管目录包含 ownership marker 和 manifest；active 指针单独存储并原子替换。安装流程必须先检查已存在版本的 manifest 和摘要，完全匹配则复用；不完整目录移入 quarantine 或清理 staging，不能当作已安装。

应用卸载与更新分开：更新永不清理持久区；显式卸载默认保留用户数据并给出单独删除选项。Skill、运行时和配置不能被 App Release Center 的强制更新覆盖。

### 3.8 新会话配置 reconciler

每次新建 Codex 或 Claude Code 会话：

1. 读取后台有效策略、用户设置和本地 inventory。
2. 生成受控字段模型：gateway、模型、MCP、Skill 搜索路径、多智能体、实时反馈和工具 PATH。
3. 解析已有 TOML/JSON/YAML，保留非受控字段。
4. 写临时文件、fsync、原子替换。
5. 回读解析并计算 fingerprint。
6. 写入会话快照和来源说明。
7. 失败则阻止 spawn，显示可操作错误和“修复配置”入口。

恢复旧会话默认保持原策略代际；只有明确可热更新的凭据和安全阻断可刷新。不得把新记忆全文重复注入旧会话。

### 3.9 消息渠道可靠性

建立统一 readiness 状态机：

```text
saved -> credential_ready -> transport_connected -> inbound_verified
      -> workspace_ready -> agent_ready -> roundtrip_ready
```

创建、启用和凭据更新后统一执行 reconcile；禁用、删除和凭据撤销统一 disconnect。配置更新必须使用 typed patch 或 merge patch，禁止 UI 重建未知字段。

端到端诊断生成 `diagnostic_id`，逐阶段记录：入站 message ID、dispatcher、路由结果、workspace、Agent resolve、spawn、prompt、TurnComplete、出站和回执。UI 显示最后成功、最后失败和修复动作。测试连接新增“完整回环测试”，同时保留低成本凭据检查。

### 3.10 用户记忆恢复

记忆写入分两条路径：

- 用户明确确认：同步 append，立即可见。
- 自动学习：TurnComplete 生成有界采集任务，抽取候选、规范化、语义去重、风险过滤后进入候选队列，达到置信阈值仍需用户确认；不得静默写入敏感推断。

状态必须可观察：最近采集、最近成功写入、最近失败、积压量、候选数量、companion health、工具是否暴露、resolved root 和文档只读状态。回退路径由 host 直接调用服务，不允许模型执行硬编码 shell 写文件。

`self-improving` Skill 负责反思和提出结构化候选；桌面 `user_memory` 是唯一持久化、权限、去重和审计层。Skill 不直接写最终文件。

### 3.11 UI 与性能

Skill 市场：

- 桌面优先的左右分栏或列表/详情布局，紧凑而可扫描；避免卡片套卡片。
- 清楚区分官方、组织、私有、强制、已安装、更新、不可兼容和准备中。
- 搜索、筛选、排序、虚拟列表、骨架、空状态、错误重试和下载进度完整。
- 安装按钮基于 resolve 状态，不能仅基于“是否存在目录”。
- 上传、安装和列表刷新使用增量状态，避免全页重取和大对象重复序列化。

后台：

- 将 Agent、工具、Skill、任务和客户端版本策略放入一致的管理信息架构。
- 表单使用选择器、分段控件、开关和版本矩阵，避免管理员手写 JSON。
- 发布前显示影响预览；高风险动作二次确认并记录审计。
- 任务控制台展示租约、尝试、checkpoint、错误、重跑和死信。
- 所有 Snowflake ID 在 JS 中保持字符串。

性能目标：

- Fusion API 不代理制品大流量。
- 目录/resolve 使用 revision + ETag，本地缓存可离线读取。
- 并发下载有全局和每主机上限，支持取消、Range 和带抖动重试。
- 大 Skill 列表虚拟化；详情和文件树按需加载。
- 初始化计划可并行下载无依赖组件，但激活严格按依赖拓扑。
- 记录应用启动、初始化、会话 spawn、Skill 安装的 P50/P95/P99。

## 4. 安全与审计

- 立即移除并轮换系统 Skill Git 硬编码凭据；检查 Git 历史和发布日志。
- 下载 URL 短时有效、绑定 artifact 和身份，不记录完整 URL。
- object key 不返回客户端；错误不泄露其他组织或私有 Skill。
- ZIP 防 Zip Slip、绝对路径、符号链接、设备文件、重复路径、大小炸弹和文件数炸弹。
- 上游镜像必须校验摘要、签名和许可证；来源变更触发 quarantine。
- 后台任务 payload/error 脱敏，不保存 token、密码或完整个人信息。
- 所有发布、策略、强制更新、重跑、取消和回滚记录不可变审计事件。

## 5. 迁移与兼容

1. 先扩展数据库和只读 API，不改变旧客户端行为。
2. 部署任务中心，回填 Skill artifacts 和工具镜像。
3. 新客户端支持初始化计划与 TOS 下载，同时保留受控旧目录导入。
4. 服务端只在统计达到门槛后关闭实时 ZIP 和旧直连字段。
5. 移除包内资源前，远端 CI 验证干净机器首次初始化、离线重启和更新保留。
6. 系统 Skill Git 链路只在新目录可回滚后停用。
7. 每阶段都有 feature flag、停止条件和回滚方法；数据库迁移只前向兼容，不回滚删除列。

## 6. 完成标准

- 复现用例中的 Skill 下载不再比较 raw size 与 ZIP size。
- Skill 大文件字节由 TOS/CDN 发送，Fusion API 只发元数据和短时票据。
- 官方、组织、用户私有和强制分发权限矩阵全部通过契约测试；组织 Skill 对同组织所有已登录用户可见。
- 干净 PC 包不含 Skill、Node、uv、Git、Agent SDK/CLI；初始化能按计划下载并原子激活。
- 更新应用后，所有配置、记忆、Skill、Agent、CLI 和运行时摘要不变。
- Codex、Claude Code 每个新会话都有 reconciler 成功 fingerprint。
- 多智能体与实时反馈新安装默认开启，旧用户显式偏好保留。
- 三类消息渠道通过真实端到端回环；失败可定位到具体阶段。
- 用户记忆有持续采集、候选、确认、失败重试和状态可视化。
- 管理员可管理版本、任务、策略、重跑、回滚和审计，无需手写 JSON。
- 健康审计中的 P0/P1 清零，P2 有明确 owner 和版本；所有关闭项有复现与回归证据。
