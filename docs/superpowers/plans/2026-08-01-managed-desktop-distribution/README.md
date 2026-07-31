# 托管桌面分发并行实施总控

## 1. 使用方式

本目录为多 Agent 并行执行入口。每个 Agent 只领取一份任务包，并在开始前完整阅读：

1. `F:\projects\iyw\iyw-claw\AGENTS.md`
2. `F:\projects\iyw\iyw-claw\AGENT.md`
3. `F:\projects\iyw\iyw-fusion-api\AGENT.md`
4. `../../specs/2026-08-01-managed-desktop-distribution-design.md`
5. 自己领取的任务文件

Fusion API 业务实现只有在用户确认本计划并明确说“开始做”后才能开始。当前文档不构成实现授权。

## 2. 全局硬规则

- 桌面仓库禁止本机编译、打包、启动、Cargo build/check/test/clippy 和 Tauri/Next build。
- 不回退、不整理、不暂存任务外脏文件；禁止 `git reset --hard`。
- 不修改任务包 `scope_write` 外文件。需要共享文件变更时生成 `integration_request`，由 Task 13 统一处理。
- SQL、共享 DTO/contract、router、bootstrap、`lib.rs`、应用总入口、根配置、CI、依赖和 lockfile 是共享资源，除 Task 01 或 Task 13 明确授权外禁止修改。
- Snowflake ID 经 JSON 全程字符串；MySQL 5.7 兼容；后端表前缀 `iyw_fusion_api_`。
- 不硬编码密钥、下载 token、稳定 TOS object key、上游凭据或用户目录。
- 每个任务先复现和记录根因，再修复；不能用吞错、取消校验、无限重试或回退直连掩盖问题。
- 新增外部调用必须有超时、重试上限、取消、结构化日志和脱敏。

## 3. 并行波次

### Wave H：P0 阻断处理，单 Agent

- Task 00：立即阻断安全、数据覆盖和确定性不可用问题。
- Task 12 同步执行 Audit A 时只能只读，不与 Task 00 同时写文件。

Wave H 只做最小安全修复和止损，不偷跑新 schema/版本中心。完成后再进入 Wave 0。

### Wave 0：共享契约，单 Agent

- Task 01：数据库、契约、迁移和 feature flag。

Wave 0 合并并冻结 `contract_revision` 后才能开始后端写任务。桌面只读审计可提前进行，但不得自行发明字段。

### Wave 1：最多四个 Agent 并行

- Task 02：持久化任务中心。
- Task 03：Skill artifact、权限、TOS 直下。
- Task 04：系统 Skill 发布源与镜像输入。
- Task 05：Agent/SDK/CLI/基础工具镜像和版本策略。

Task 02/03/05 禁止修改共享 router/bootstrap；各自导出装配函数，由 Task 13 接线。Task 04 只修改 `skill` 发布源。

### Wave 2：最多四个 Agent 并行

- Task 06：桌面包瘦身、初始化、本地库存和升级保护。
- Task 07：新会话配置、多智能体和实时反馈默认策略。
- Task 08：消息渠道端到端修复。
- Task 09：用户记忆与 self-improving Skill 闭环。

Task 06 依赖 Task 03/05 的冻结 API。Task 07-09 可与 Task 06 并行，但共享 ACP 接线全部留给 Task 13。

### Wave 3：最多三个 Agent 并行

- Task 10：桌面 Skill 市场 UI 和性能。
- Task 11：后台管理 UI 与任务控制台。
- Task 12：安全、可观测性、性能和全项目健康审计。

Task 10 依赖 Task 03 的权限和状态枚举，但只修改前端 Skill 市场文件；Task 11 依赖 Task 02/03/05 的管理 API。

### Wave 4：单一集成 Agent

- Task 13：跨任务接线、迁移、回归、发布和双远端交付。

## 4. 文件所有权

| 区域 | 唯一 owner |
| --- | --- |
| P0 已知断链的最小止损文件 | Task 00；必须在其他写任务前完成 |
| `scripts/mysql/*`、共享 API schema/DTO | Task 01 |
| `jobcenter` 新领域、应用、MySQL 适配 | Task 02 |
| Fusion `skill` 领域/应用/仓储/用户与管理 handler | Task 03 |
| `skill` 仓库发布校验、清单和 CI | Task 04 |
| Fusion `agentrelease` 领域/应用/仓储/handler | Task 05 |
| 桌面 runtime、version center installer、NSIS、Tauri 资源配置 | Task 06 |
| provider overlay、session config、delegation/feedback settings | Task 07 |
| `chat_channel`、chat channel commands/db/UI | Task 08 |
| `user_memory`、记忆 companion bridge、记忆设置 UI、self-improving 契约 | Task 09 |
| `src/components/skills`、Skill 市场页面与纯前端数据层 | Task 10 |
| `iyw-fusion-api/docs/admin` 管理前端 | Task 11 |
| 审计文档、基准脚本和非业务诊断工具 | Task 12 |
| router/bootstrap/lib.rs/root config/CI/lockfile/跨域冲突 | Task 13 |

任何两个并行任务不得修改同一文件。发现现有所有权不够时停止写入并上报，不通过抢改共享文件解决。

## 5. 推荐 branch/worktree 布局

并行实现优先为每个任务创建独立 worktree。Task 01 先合并到集成基线，再从同一基线创建 Wave 1 分支；不要让多个 Agent 在同一 index 中提交。

| Task | 仓库 | 建议分支 |
| --- | --- | --- |
| 00 | `iyw-claw` | `fix/managed-t00-blockers` |
| 01 | `iyw-fusion-api` | `feat/managed-t01-contract` |
| 02 | `iyw-fusion-api` | `feat/managed-t02-job-center` |
| 03 | `iyw-fusion-api` + `iyw-claw` | 两仓同名 `feat/managed-t03-skill-artifact` |
| 04 | `skill` | `feat/managed-t04-release-source` |
| 05 | `iyw-fusion-api` | `feat/managed-t05-component-mirror` |
| 06 | `iyw-claw` | `feat/managed-t06-bootstrap` |
| 07 | `iyw-claw` | `feat/managed-t07-session-config` |
| 08 | `iyw-claw` | `fix/managed-t08-channels` |
| 09 | `iyw-claw` | `fix/managed-t09-memory` |
| 10 | `iyw-claw` | `feat/managed-t10-skill-ui` |
| 11 | `iyw-fusion-api` | `feat/managed-t11-admin-ui` |
| 12 | 只读三仓，报告落 `iyw-claw` | `audit/managed-t12-health` |
| 13 | 三仓集成 worktree | `feat/managed-t13-integration` |

Task 03 是唯一正常情况下需要同时提交两个仓库的子任务；handoff 必须分别给出两仓 commit。子任务只提交自己的 branch，不 merge/rebase/push 主分支，不删除 worktree；Task 13 统一集成和推送。

## 6. Agent 启动提示词

复杂跨模块任务推荐 `gpt-5.4` + `xhigh`；Task 04 的发布校验可用 `gpt-5.3-codex` + `high`。启动一个 Agent 时使用：

```text
执行 F:\projects\iyw\iyw-claw\docs\superpowers\plans\2026-08-01-managed-desktop-distribution\<任务文件>。

严格先读该文档“使用方式”列出的 AGENT/AGENTS 和总体设计。只修改 task 的 scope_write；共享文件只提交 integration_request，不越界。开始前记录三仓 git status，不回退现有改动。先复现并登记缺陷，再实现，再按任务验证；桌面端禁止在本机编译、运行或测试。完成时提交 handoff YAML、实际验证证据和本任务 branch commit，不 merge/rebase/push 主分支。

如果发现 schema/shared DTO/router/bootstrap/root config/CI/lockfile 或其他任务 owner 文件必须变化，立即停止该部分写入，状态标 BLOCKED，并给出所需字段、调用点和理由。
```

同一 Wave 的 Agent 使用不同 worktree 和 branch。每个 Agent 开始时把自己的任务标为 `in_progress`，完成后只更新自己的 handoff；总控状态由 Task 13 维护。

## 7. 统一交付记录

每个任务完成时提交 `handoff`，至少包含：

```yaml
task_id: Txx
status: complete|blocked|partial
contract_revision: <revision>
changed_files: []
migrations: []
new_routes: []
feature_flags: []
verification:
  commands: []
  results: []
known_risks: []
integration_requests: []
rollback: <method>
```

缺陷台账使用：

```yaml
id: IYW-AREA-NNN
severity: P0|P1|P2|P3
status: open|confirmed|fixed|verified|deferred
symptom: <用户可见现象>
environment: <版本/平台/组织/渠道>
reproduction: []
evidence: []
root_cause: <代码级原因>
owner_task: Txx
fix_commit: <sha>
regression_evidence: []
```

## 8. 质量门禁

- P0/P1 先修且必须有回归用例；P2 进入明确版本；P3 进入可检索 backlog。
- Fusion API：定向单元/仓储/HTTP contract 测试，通过后再跑相关包和全量测试。
- `skill`：校验 `experts.toml`、依赖无环、每个 Skill 有 `SKILL.md`、稳定标签与 bundle version 一致。
- 桌面：本机只做静态调用链审查、格式/文档检查和 `git diff --check`；编译、安装包检查和 E2E 必须由远端 CI/发布机提供证据。
- UI：远端 Playwright 覆盖 1280x800、1440x900、1920x1080 和窄屏；检查无重叠、无横向溢出、长文本可读、键盘焦点和错误状态。
- 下载：故障注入覆盖断网、URL 过期、Range 不支持、摘要错、签名错、磁盘满、进程退出、重复启动和并发计划。

## 9. 任务索引

1. `00-immediate-blockers.md`
2. `01-contract-schema-migration.md`
3. `02-persistent-job-center.md`
4. `03-skill-artifact-permission-distribution.md`
5. `04-system-skill-publishing.md`
6. `05-agent-tool-version-mirroring.md`
7. `06-desktop-bootstrap-upgrade-protection.md`
8. `07-session-config-defaults.md`
9. `08-message-channel-reliability.md`
10. `09-user-memory-skill-learning.md`
11. `10-desktop-skill-market-ui.md`
12. `11-admin-console.md`
13. `12-health-security-performance-audit.md`
14. `13-integration-rollout-delivery.md`
