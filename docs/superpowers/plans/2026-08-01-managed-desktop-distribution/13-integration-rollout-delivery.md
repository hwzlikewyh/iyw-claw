# Task 13：跨仓集成、迁移、发布与交付

## 目标

作为唯一集成 Agent，处理共享入口、跨任务 contract 对齐、feature flag、端到端验证、分阶段上线、提交和双远端推送。子任务完成不等于项目完成。

## 前置

- 收集 Task 00-12 的 handoff、integration request、contract revision 和缺陷台账。
- 所有工作树先记录状态；不覆盖任何用户脏改动。
- Fusion 实施必须已有用户明确“开始做”授权。

## 独占 scope_write

- Fusion：router、bootstrap、应用总入口、根配置、环境模板、共享 API 聚合和 migration 注册。
- 桌面：`lib.rs`、web router、`commands/acp.rs`/ACP manager/connection 的跨域接线、设置总入口、Tauri 根 config 的最终合并、CI。
- 根依赖/lockfile（仅确有必要且单独说明）。
- 跨任务冲突文件和发布文档。

## 集成顺序

1. 核对所有任务使用同一 contract revision；拒绝私自扩展字段的 handoff。
2. 合并 Task 01 schema，先在空库和旧库验证。
3. 装配 job center，但不开启业务 handler。
4. 接 Skill artifact build 和历史回填，先 shadow/dry-run。
5. 接 Agent/tool mirror，先只发现和构建 draft。
6. 发布有真实 ready artifact 的 catalog。
7. 接桌面 bootstrap plan、inventory 和 installer，feature flag 默认灰度。
8. 接 SessionConfigReconciler 到所有 new conversation spawn。
9. 接 channel reconcile/readiness/roundtrip diagnostics。
10. 接 memory TurnComplete harvest 和完整设置 snapshot。
11. 启用新 Skill UI/admin UI。
12. 执行 Audit B、清 P0/P1，再扩大灰度。

## 共享接线检查

### Fusion

- job worker 在 bootstrap 启动/关闭，固定并发和 drain。
- routes 按 user/admin auth 分组，CORS/timeout/body limit 正确。
- TOS、签名、Git/registry 凭据只从配置注入。
- runtime revisions 对 Skill/agent/tool/policy 变更递增且快照拒绝倒退。
- 原 ticker 在新 handler 稳定后关闭，不能新旧同时执行。

### 桌面

- UI、channel、subagent 的所有新会话入口都经过 storage -> bootstrap readiness -> config reconciler -> spawn。
- 恢复/probe 走各自明确分支。
- system/market/user Skill 搜索路径不重叠。
- App updater 仅触碰 app；初始化状态在窗口间共享。
- web/server runtime 与 Tauri command 尽量复用 `_core`，避免桌面修好但服务端断裂。

## Feature flags

至少具备：

- `skill_artifact_v2_read`
- `skill_tos_direct_download`
- `persistent_job_workers`
- `managed_component_bootstrap`
- `system_skill_catalog`
- `session_config_reconcile_required`
- `channel_reconcile_v2`
- `memory_harvest`

每个 flag 记录 owner、默认值、依赖、监控、回滚和删除版本。安全修复不可长期被 flag 关闭。

## 数据迁移

- audience 回填先 dry-run 统计，再分批写。
- Skill artifact backfill 限速，不和生产 relay 热路径争抢资源。
- 客户端旧目录导入有 receipt，失败不删除源。
- 默认 delegation/feedback 迁移仅处理缺失键。
- 硬编码凭据先轮换，再删除代码；提交历史清理如需重写必须单独获得用户授权，不能擅自 force push。

## 发布闸门

### Gate A：后端暗发布

- schema/worker/API 上线，客户端不可见。
- build artifact、镜像和权限 shadow 比较。
- 无 lease duplication、越权和 TOS 大流量代理。

### Gate B：内部客户端

- 小比例 installation ID 开启新 bootstrap/Skill 下载。
- 监控 resolve、下载、校验、激活、回滚和启动耗时。
- 三渠道和 memory 在内部账号 E2E。

### Gate C：组织灰度

- 1% -> 10% -> 30% -> 100%，每阶段至少覆盖一个完整更新周期。
- P0 立即停止；P1 超阈值回滚 flag；推荐版本不做自动降级。

### Gate D：移除旧链路

- 旧实时 ZIP、系统 Skill Git pull、包内 runtime/Skill、外部正常下载路径的使用量为零。
- 至少保留一个客户端兼容周期后删除代码和 flag。

## 最终验收矩阵

- 全新安装、已有安装升级、离线重启、强制最低安全更新、pin/LKG/rollback。
- Skill 官方/组织/私有/强制、依赖、历史版本、断点续传和 URL 刷新。
- Node/uv/Git/Agent SDK/CLI 已有版本跳过和 PC 版本强制更新。
- Codex/Claude Code 连续新会话配置 fingerprint；旧会话策略保持。
- delegation/feedback 新用户默认开、旧用户显式 false 保留、kill switch 生效。
- 企微/飞书/微信完整对话和故障诊断。
- memory 多轮采集、候选确认、新会话读取和失败恢复。
- App 更新前后 Skill/Agent/CLI/runtime/config/data/memory 摘要一致。
- 管理页版本/策略/任务/审计可用，Snowflake ID 无精度损失。

## 验证命令与证据

- Fusion：gofmt、定向 tests、全量 Go tests、race（适用包）、contract/integration、migration diff。
- `skill`：manifest/依赖/标签校验。
- 桌面：当前机器不编译；远端 CI 执行 Rust/TS lint/build/test、安装包内容、Windows E2E 和 Playwright 截图。
- 所有命令记录实际退出码和 artifact/log 链接；未执行不得写“通过”。

## 提交与推送

- 每仓独立提交，消息格式 `<type>(scope): <中文动词摘要>`。
- 只暂存任务文件；提交前逐项 `git diff --cached --name-only`。
- 先 fetch 并检查非快进，不使用 reset/force/rebase 覆盖用户历史。
- `iyw-claw` 使用 `git push origin <branch>`，核验 origin 的 GitLab 与 GitHub 两个 push URL 都成功。
- 其他仓库先检查真实 remote；缺 GitHub remote 时报告阻塞，不擅自添加或猜地址。
- 推送失败保留本地提交和工作区，记录哪个远端成功/失败。

## 回滚

- 优先关 feature flag、暂停 policy/worker 和恢复旧 active/LKG。
- 不删除新表/列，不覆盖客户端持久目录，不自动降级健康版本。
- security block 不能因普通回滚失效。

## 完成定义

- Task 00-12 handoff 全部核对，integration request 清零。
- Audit B 的 P0/P1 清零，完整验收矩阵有证据。
- 三仓提交边界清晰，桌面双远端推送均核验。
- 旧链路只在满足使用量和兼容窗口条件后移除。
