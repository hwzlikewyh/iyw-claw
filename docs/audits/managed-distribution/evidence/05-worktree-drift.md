# 工作区漂移复核（2026-08-01 复核时点）

# Audit A 基线提交：iyw-claw b46a4c4 / iyw-fusion-api d0df3bc / skill ea76a4e
# 复核发现：三仓工作区出现大量未提交并行任务改动（T00/T01/T03/T04/T05/T06/T07/T08/T09 相关），
# 部分种子缺陷在工作区已有未提交修复。本文件记录漂移事实；缺陷状态更新见 defects.yaml。

## iyw-claw（当前分支 audit/managed-t12-health；另有 feat/managed-t07-session-config、fix/managed-t09-memory）
- system_skills/git.rs：已删除硬编码凭据（inject_system_skills_credentials），改为 inject_credentials_for_url/require_origin_credentials（DB 凭据）=> IYW-SEC-001 工作区已修
- system_skills/manager.rs：2026-08-01 复核时曾由 force_reset 改为“auto update stopped”+mark_dirty；2026-08-05 产品决策改回强制覆盖（dirty 仅记日志后继续 force_reset），BlockedDirty 状态与启动自动更新开关均已移除 => IYW-SKILL-002 转为 decision accepted（详见 defects.yaml）
- chat_channel/：新增 config_patch.rs、dedupe.rs、diagnostics.rs、readiness.rs、reconcile.rs、db 迁移 m20260801_000001_chat_channel_reliability.rs；manager.rs 改为 reconcile_all_enabled => IYW-CHANNEL-001..006 工作区已修
- src/lib/chat-channel-config.ts：新增 buildChatChannelConfigPatch（保留 workspace_root/未知字段）=> IYW-CHANNEL-004 前端已修
- user_memory/harvest.rs：新增（TurnComplete 采集）=> IYW-MEMORY-001 工作区部分实现
- user_memory/context.rs:84：仍保留 C:/Users/Administrator 硬编码路径 => IYW-MEMORY-002 未修
- acp/session_config_reconciler/：新增目录 => IYW-CONFIG-001 工作区有实现
- acp/version_center/installer/*：新增 activation/component/init/manifest/migration/preflight/resumable/state.rs => IYW-DIST-001 工作区有实现
- tauri.conf.json、prepare-sidecars.mjs、installer-hooks.nsh、runtime_bootstrap.rs、binary_cache.rs 均有改动 => IYW-DIST-001 工作区有实现

## iyw-fusion-api（当前分支 master；另有 feat/managed-t01-contract、feat/org-skill-management）
- docs/contract/managed-distribution-contract.md + managed-distribution.openapi.yaml：新增（Task 01 契约文档）
- internal/domain/skill/artifact.go、audience.go：新增（Task 03 契约实现）
- internal/domain/skill/access.go：复核时正被其他进程编辑（锁住未读取）
- internal/application/agentrelease/mirror_*.go、plan_service.go、upstream_*.go：新增（Task 05 镜像/计划）
- docs/admin/admin-console.*：新增（Task 11 管理控制台）
- scripts/mysql/031/032/033 迁移仍为未跟踪（未提交）

## skill（当前分支 feat/managed-t04-release-source）
- 已存在 feat/managed-t04-release-source 分支（Task 04 发布源工作分支）

## 结论
- Audit A 基线快照仍有效（针对 b46a4c4/d0df3bc/ea76a4e），但缺陷状态需按工作区当前事实更新
- 未提交改动不能作为 verified 依据（无回归证据、未合并），统一标 worktree_fix: present（未提交、未验证）


## 复核时点二（2026-08-01 会话期间，HEAD 继续前进）
- iyw-claw 工作分支被并行 agent 切换：audit/managed-t12-health -> feat/managed-t06-bootstrap（两分支同指 fd929fa，含 Audit A 两个提交）；未提交改动增至 183 个文件。
- iyw-fusion-api HEAD：7b1c0a8 -> fdead55 feat(contract): 冻结 Skill 受众与制品 DTO 枚举（Task 01/03 继续提交，均未推送）。
- skill HEAD：ea76a4e -> e811dbb（main），branch 切回 main；experts.toml version 0.0.11 vs 标签最高 v0.0.8 不变（IYW-SKILL-013 未修）。
- 复核确认（工作区未提交）：commands/experts.rs include_dir! 16 个（IYW-SKILL-003 未修）；install.rs 仍 package_size/object_sha256 校验（IYW-SKILL-001/008 未修）；update/version.rs:19,24 仍直连 GitHub（IYW-DIST-002 未修）；context.rs:84 Administrator 仍在（IYW-MEMORY-002 未修）；user-memory-settings.tsx:29,78 sanitize 仍在（IYW-MEMORY-003 未修）；skill-market-data-list.tsx 无虚拟滚动（IYW-UI-001 未修）。
- 新增证据：evidence/06-static-audit-notes.md（NSIS/阻塞IO/跨await锁/忽略Result/SQL/重试超时/行号对照）；evidence/07-secret-scan-recheck.txt（复核扫描：0 处真实凭据，1 处测试夹具假值）。
- 新增脚本：scripts/check_defects_references.ps1、scan_secrets.ps1、audit_b_recheck.ps1（Audit B 复用，纯 PowerShell，无新增依赖）。


## 复核时点三（2026-08-01 并行任务已各自提交实现分支）
- iyw-fusion-api 当前分支 feat/managed-t02-job-center@8d48ed9：租约式持久化任务中心实现（domain jobcenter lease/retry/status/progress/metrics + MySQL repository + bootstrap 装配函数）；未接入 bootstrap.go（Task 13 接线），进程内 ticker（agent_release.go:73-81、skill_upload_cleanup.go:20-29）仍在 => IYW-JOB-001 branch_fix
- iyw-fusion-api contract 系列提交（7b1c0a8/fdead55/0b6ad7d/36cec1d）冻结 audience/artifact/组件绑定枚举与 031-033 迁移 => IYW-AUTH-001/IYW-SKILL-005/006/008 branch_note
- iyw-claw feat/managed-t06-bootstrap@c65425b：桌面包瘦身与托管初始化（prepare-sidecars.mjs 大幅精简、tauri.conf.json、runtime_bootstrap.rs 重写、version_center/installer 模块化）=> IYW-DIST-001 branch_fix
- skill main@ba3ca56（ahead 4）：发布校验与清单流水线（verify_manifest.py 强制 version/tag/size/sha256 一致、package_release.py 确定性源码包）=> IYW-SKILL-013 branch_fix
- 共享主工作树 iyw-claw 已被并行 agent 使用（feat/managed-t06-bootstrap）；审计交付物已从主工作树清理，仅存于 audit/managed-t12-health 分支
- 结论：Task 02-11 实现均已进入各自分支但尚未集成（Task 13 未接线）；Audit B 前置条件（合并后回归）仍未满足


## 复核时点四（2026-08-01 并行任务继续提交）
- iyw-claw feat/managed-t10-skill-ui@3778b16：重构 Skill 市场 UI（market/* 组件 + virtua Virtualizer 虚拟滚动）=> IYW-UI-001 branch_fix
- iyw-claw feat/managed-t07-session-config@cd61e42/5204d96/87083be（worktree iyw-claw-t07）：新会话配置对账 reconciler + delegation/feedback 默认策略 => IYW-CONFIG-001、IYW-DEFAULT-001 branch_fix
- iyw-claw feat/managed-t06-bootstrap@3724184：下载 resume meta 绑定 expected_size/sha256 + MAX_SINGLE_FILE_BYTES=512MiB => IYW-DIST-001 补充
- fusion-api 仍为 feat/managed-t02-job-center@8d48ed9（T01 contract 提交并入该分支祖先）；skill main@e811dbb（T04 已合并）
- T08（渠道）/T09（记忆）尚未提交分支（工作区未提交实现仍在）；无集成基线（Task 13 未接线）
