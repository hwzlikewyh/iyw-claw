# Task 01：共享契约、数据库与迁移

## 目标

一次性冻结 Skill artifact、权限、任务中心、统一初始化计划和版本策略的共享数据契约。此任务只建立 schema、共享 DTO/枚举和兼容策略，不实现 worker、下载或 UI。

## 前置与依赖

- 必须先完成总控 README 的五份前置阅读。
- 复核现有 `scripts/mysql/025` 到 `030`、`docs/skill-market-user-client-integration.md`、Agent Version Center spec/plan 和实际 handler DTO。
- MySQL 版本固定为 5.7；不能使用 `SKIP LOCKED`、CTE、窗口函数或 8.0 专有语法。
- 该任务完成并给出 `contract_revision` 前，Task 02/03/05 只允许只读分析。

## scope_read

- `iyw-fusion-api/scripts/mysql/001_init_schema.sql`
- `iyw-fusion-api/scripts/mysql/011_create_latest_schema_for_prod.sql`
- `iyw-fusion-api/scripts/mysql/023_create_org_skills.sql` 到 `030_seed_agent_platform_registry.sql`
- `iyw-fusion-api/internal/domain/skill/`
- `iyw-fusion-api/internal/domain/agentrelease/`
- `iyw-fusion-api/internal/application/skill/`
- `iyw-fusion-api/internal/application/agentrelease/`
- `iyw-fusion-api/internal/adapter/httpserver/skill*`
- `iyw-fusion-api/internal/adapter/httpserver/agentplatform*`
- 桌面 `src/lib/skill-market.ts` 与 `src-tauri/src/acp/version_center/`

## scope_write

- 新增顺序迁移脚本，建议：
  - `031_create_persistent_job_center.sql`
  - `032_upgrade_skill_artifacts_and_audience.sql`
  - `033_extend_managed_component_policies.sql`
- 更新 `001_init_schema.sql` 和 `011_create_latest_schema_for_prod.sql` 的最新全量结构。
- 新增一个共享 contract 文档或 OpenAPI 片段；若项目已有同类生成源，沿用该源。
- 只在必要时调整纯 DTO/枚举文件；业务判断留给对应任务。

## 禁止修改

- 后端 router/bootstrap、worker、TOS 适配器、业务 service 和管理 UI。
- 桌面 Rust/React 业务代码。
- 根依赖、lockfile、CI。

## 数据模型

### 持久化任务

新增 `iyw_fusion_api_background_jobs`：

- `id BIGINT`：应用侧 Snowflake。
- `job_type VARCHAR(64)`、`dedupe_key VARCHAR(255)`。
- `status`：`pending/running/succeeded/failed/dead/cancelled`。
- `priority INT`、`scheduled_at DATETIME(3)`、`next_retry_at DATETIME(3)`。
- `lease_owner VARCHAR(128)`、`lease_until DATETIME(3)`、`fencing_token BIGINT`。
- `attempt INT`、`max_attempts INT`。
- `payload_json MEDIUMTEXT`、`checkpoint_json MEDIUMTEXT`，均设业务上限。
- `progress_current/progress_total BIGINT`。
- `last_error_code VARCHAR(64)`、`last_error_detail VARCHAR(2048)`。
- `created_by/updated_by`、开始/结束/创建/更新时间。
- 唯一索引 `(job_type, dedupe_key)`；抢占、重试、列表和清理索引。

新增 `iyw_fusion_api_background_job_events`：状态变化、attempt、fencing token、脱敏摘要和时间。事件不可变，不保存 payload 全文。

### Skill 受众与分发

在 Skill 上新增或迁移：

- `audience = global_market | organization | owner_private`。
- 保留 `publisher_type = official | user`。
- `distribution_policy = mandatory | optional` 不放入 audience。
- `org_code` 对 organization 必填；global official 可使用平台 org，但授权不依赖相等。

迁移映射必须显式：

- 现有官方 public -> `global_market`。
- 现有用户 private -> `owner_private`。
- 现有用户 public 不可直接猜成 global；根据产品确认映射为 `organization`，并生成迁移统计。

### Skill artifact

优先新增独立 `iyw_fusion_api_skill_artifacts`，不要继续重载 `skill_versions.package_size`：

- `id`、`skill_id`、`version_id`、`generation`。
- `status`、`raw_size`、`artifact_size`。
- `artifact_sha256`、可选 `signature`。
- `object_key`、`file_name`、`package_kind=zip`。
- `build_job_id`、`failure_code`、`verified_at`、时间字段。
- 唯一 `(version_id, generation)`；ready object key/sha 索引。

Skill version 保留原始内容摘要；增加 `active_artifact_id` 或通过 ready generation 选择。旧 `package_size` 标为 legacy，兼容期只读，不再返回给新客户端作为 ZIP 大小。

### Skill 定向策略

新增策略表或复用明确的 policy 聚合，字段至少包含：

- skill/version、mandatory/optional。
- org 定向、channel、target、arch。
- `min_client_version`、可选 `max_client_version`。
- rollout mode/basis points/start/pause。
- enforce after、status、审计主体。

同一有效范围内禁止互相矛盾的强制版本。

### 托管组件扩展

扩展现有 `agent_tools`/versions/artifacts，而非新建平行工具表：

- 工具 ID allowlist 至少含 `git/node/uv`，并为 `agent_sdk/cli/auxiliary_cli` 定义受信 registry ID。
- 增加 PC 客户端兼容范围、依赖关系、来源版本、许可证摘要和 mirror job 关联。
- 发布后的 artifact 字段不可变。

### PC 应用版本关联

PC 应用版本继续以现有 App Release Center 为准，不创建第二套 PC release 表。扩展关系必须能表达：

- 一个 `iyw-claw` release 发布时绑定经过验证的 component policy/catalog revision。
- 该 PC 版本启动所需的最低 Node、uv、Git、Agent/SDK/CLI 和系统 Skill 范围。
- 应用 `optional/required/enforceAfter/requiredBelowVersion` 与组件 `recommended/minimum-safe/mandatory` 相互独立。
- 发布校验保证目标平台的必要组件已有 ready artifact；不满足时 PC release 不能发布。
- App 更新计划与 component bootstrap plan 使用同一个 installation/client context，避免先更新 App 后才发现必要组件不可用。

## API 契约

### Skill 安装计划 v2

请求至少包括：Skill ID/版本、客户端版本、channel、target、arch、本地 inventory revision。

响应每项必须返回：

- 字符串 `skillId/versionId/artifactId`。
- `version`、`audience`、`distributionPolicy`。
- `artifactSize`、`artifactSha256`、可选 signature。
- `ticketEndpoint`，不返回 object key。
- dependency order 和 catalog revision。

`artifactSize` 是 ZIP 实际字节；`rawSize` 只允许作为可选展示统计，客户端不得用于下载完整性判断。

### 下载票据

输入：artifact ID、plan ID、客户端上下文。输出：短时 URL、过期时间、允许 Range、同一 artifact metadata。票据刷新不重新 resolve 版本。

### 桌面初始化计划

请求：installation ID、client version/channel、target/arch、org、inventory、能力 schema version。

响应：`planId/catalogRevision/expiresAt` 和拓扑排序 actions。action 固定为 `keep/install/update/block/remove_managed`，包含 component kind/id/version/reason/artifact metadata/ticket endpoint/mandatory deadline。

### 兼容错误

统一机器可读 code：`artifact_not_ready`、`client_incompatible`、`audience_denied`、`dependency_unavailable`、`version_blocked`、`plan_expired`、`catalog_stale`。权限错误不泄露不可见对象是否存在。

## 实施步骤

1. 生成当前表结构和索引清单，确认没有同名字段/索引。
2. 编写增量迁移，所有 ALTER 可重复检测或有明确一次性执行说明。
3. 更新两个全量 schema，保证新库和升级库等价。
4. 编写数据回填 SQL/脚本和 dry-run 统计；大表分批，不做长事务全表锁。
5. 固定 JSON 字段、枚举、大小单位、时间格式和 ID 字符串规则。
6. 为旧客户端定义兼容窗口：旧端继续看到旧 endpoint；新端只认 artifact v2。
7. 输出 `contract_revision` 和字段变更表，通知其他任务。

## 验证

- 在临时 MySQL 5.7 上分别执行“空库全量建表”和“旧结构逐步迁移”。
- 比较两者表、列、默认值、注释和索引。
- 验证 Snowflake ID 无自增、JSON ID 示例均为字符串。
- 验证 audience 回填数量总和等于迁移前 Skill 数，异常记录为零或有人工清单。
- 验证 SQL 不包含 MySQL 8 专有语法。
- 运行受影响领域的 schema/DTO 定向测试；不得声称桌面编译通过。

## 完成定义

- 契约已冻结且可供 Task 02/03/05 实现。
- 新旧库等价证据、回填统计、回滚策略和兼容截止版本齐全。
- 没有把业务 worker 或路由接线混入本任务。

## 回滚

上线前只回滚代码；上线后不删除新增列/表。关闭 feature flag，恢复旧读路径，保留新数据等待修复后重启迁移。
