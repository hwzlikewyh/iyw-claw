# 验证矩阵（verification-matrix.md）

> Audit A 只读基线 · 2026-08-01 · 每条证据必须能回到真实命令/文件/源码位置。
> 工作区漂移（2026-08-01 复核）：三仓存在未提交并行任务改动，部分缺陷工作区已有修复但未合并、未验证，一律不视为 verified；详细对照见 evidence/06-static-audit-notes.md §8。
> 桌面动态证据一律来自远端 CI/测试机；当前机器未编译、未启动任何桌面产物。

## A. 已执行静态验证（Audit A）

| 类别 | 命令/方式 | 结果 | 证据 |
| --- | --- | --- | --- |
| 三仓基线 | `git status --porcelain -b` + `git log --oneline -5` | 基线 iyw-claw b46a4c4 / fusion-api d0df3bc / skill ea76a4e；复核时 fusion-api HEAD 已前进 7b1c0a8（Task 01 契约/迁移本地提交未推送）；均有未提交并行任务改动（不动） | 见本目录 evidence/00-* 与 evidence/05-worktree-drift.md |
| Secret 扫描 | rg 凭据模式（已脱敏输出） | 基线命中 IYW-SEC-001（git.rs:48）；复核时工作区已删除硬编码凭据（改 DB 凭据注入），历史 3b6e21b 清理仍待 Task 00/13 | `evidence/03-credential-pattern-masked.txt`；security.md §1 |
| 外部 URL 清单 | `rg -o 'https?://...'` | 桌面 50+ 直连上游 URL 已登记 | `evidence/02-desktop-external-urls.txt`；inventory.md §3 |
| Rust 风险模式 | `rg '\.unwrap\(\)|\.expect\(|panic!|unreachable!'` | 118 处；抽查 web/mod.rs（锁）、remote_image（长度守卫）、terminal/manager.rs（锁）未见用户输入直触 panic | `evidence/01-rust-unwrap-expect.txt` |
| include 内嵌 | `rg 'include_dir!|include_bytes!'` | experts.rs 14 个 Skill 内嵌（IYW-SKILL-003）；tray icon include_bytes | reliability/security |
| SQL 注入面 | `rg 'fmt.Sprintf|\.Raw\(|\.Exec\(|\.Query\('` | 未发现把用户输入拼 SQL；GORM 参数化 Where 为主；admin.go:131 有静态 Exec（改配置） | 扫描记录（fusion-api） |
| 日志脱敏 | 读 relay http_utils.go / record_body.go / upstream_logging.go | preview 512B 上限 + RedactedPreview + imageErrorPreview；测试断言不泄漏 | security.md §1.2 |
| 前端 Snowflake | `rg 'parseInt\(|Number\('` | 命中点均为本地小整数（folderId/attempt/zoom）；Skill 市场用 string id | security.md §2.2 |
| 前端竞态 | 读 skill-market-data-list.tsx | 有 requestId 防旧响应覆盖 + 防抖 + 分页（正向） | reliability.md §6 |
| 管理页鉴权 | 读 admin/admin.go | `/admin/api` 有 adminAuth；`/admin/*` 静态页无鉴权（P3，IYW-AUTH-002） | security.md §2.1 |
| 渠道断链 | 读 manager.rs / reconcile.rs / chat_channel.rs / edit-chat-channel-dialog.tsx | 基线确认 IYW-CHANNEL-001..006；复核时工作区已接入 reconcile（create/update/connect）、企微去 token gate、编辑走 config patch；回环与 readiness UI 未验证 | reliability.md §3；defects.yaml worktree_fix |
| 记忆断链 | 读 context.rs / user-memory-settings.tsx / harvest.rs | 基线确认 IYW-MEMORY-001..003；复核时工作区新增 harvest.rs（MEMORY-001 部分实现）；context.rs:84 Administrator 仍存在（未修） | reliability.md §4；defects.yaml |
| 系统 Skill | 读 git.rs / manager.rs | 基线确认 IYW-SEC-001、IYW-SKILL-002；git.rs 已去硬编码；manager.rs dirty 分支按 2026-08-05 决策保持 force_reset（BlockedDirty 已移除） | defects.yaml；evidence/06 §8 |
| skill 发布源 | 读 experts.toml + `git tag` | bundle.version=0.0.11 vs 标签最高 v0.0.8（IYW-SKILL-013） | defects.yaml |

| NSIS 安装/更新覆盖 | 读 windows/installer-hooks.nsh + update/install.rs | 工作区版本：app 区替换 + canonicalize 校验、更新跳过卸载确认；普通卸载选择保留用户数据时清理程序目录，/PURGE 全量删除；update 走原子 rename+fsync+备份 | evidence/06 §1-2（正向） |
| async 阻塞 IO | rg spawn_blocking + 读热路径 | 26 处 spawn_blocking 覆盖文件/压缩/备份/文件锁；未见 async 热路径大文件 IO | evidence/06 §3（正向） |
| 跨 await 锁 | 启发式扫描 + 抽查 | std Mutex 53 文件 / tokio Mutex 50 文件；候选 164 处（误报高）；抽查 web/mod.rs、terminal/manager.rs 未见持锁跨 await | evidence/06 §4（待 Audit B 工具确认） |
| 忽略 Result | rg 'let _ = ' | 热点范围 163 处；weixin.rs:742 出站 do_send 被忽略（IYW-CHANNEL-006 佐证）；日志写失败静默 | evidence/06 §5 |
| 重试/超时边界 | rg ATTEMPTS/.timeout/loop | 有界重试为主（PACKAGE_DOWNLOAD_ATTEMPTS=3、NPM_INSTALL_ATTEMPTS=3、MAX_ATTEMPTS=3）；.timeout 23 处；无计数 loop 55 处需动态验证 | evidence/06 §6 |
| SQL 索引/参数化 | 读 scripts/mysql/* + GORM 查询 | 全表主键+查询键唯一索引；031-033 已随 7b1c0a8 提交；无用户输入拼 SQL | evidence/06 §7（正向） |
## B. 未执行 / 无法在本机执行的验证（原因与责任人）

| 类别 | 原因 | 责任人 | 目标 |
| --- | --- | --- | --- |
| 桌面编译/启动/打包/E2E | 本机禁止（0801README 硬规则） | 远端 CI/发布机 | Audit B |
| 渠道真实账号回环 | 需要三渠道账号与运行环境 | T08 + 远端 | Audit B |
| TOS/MySQL/Git 故障注入 | 需要隔离运行环境 | T12（脚本）+ T13 | Audit B |
| Fusion unit/contract/race/benchmark | 需隔离环境；Task 01 契约/迁移已随 7b1c0a8 本地提交但未推送、并行实现未合并，当前无集成后可测代码；存量测试可在隔离环境跑 | T13 合并后 | Audit B |
| 内存/句柄/goroutine 泄漏 | 需要运行与压测 | 远端 CI | Audit B |
| 前端 Playwright（1280x800/1440x900/1920x1080/窄屏） | 需要远端浏览器 | 远端 CI | Audit B |

## C. 复现命令（供各 owner 与 Audit B 使用）
```powershell
# 1. 复现 IYW-SKILL-001/008（Fusion 隔离环境 + 桌面远端）
#    上传约 19644 字节原始文件的版本 -> POST /skills/download -> 观察 expected/received 不一致与 objectSha256 失败
# 2. 复现 IYW-SKILL-002（桌面远端 fixture）
#    在系统 Skill 目录制造 dirty 修改 -> 触发更新 -> 观察 force reset 前后文件摘要变化
# 3. 复现 IYW-SEC-001（只读）
git -C iyw-claw show 3b6e21b -- src-tauri/src/system_skills/git.rs
# 4. 复现 IYW-CHANNEL-003（桌面远端）
#    新建企微渠道（不存 channel token）-> auto-connect/connect -> 观察 token gate 拦截
# 5. 复现 IYW-MEMORY-002
#    在非 Administrator 用户下触发 MCP 记忆路由失败 -> 观察提示是否仍写 C:/Users/Administrator/...

# 6. 引用与漂移复核（Audit A 补充，脚本见 scripts/）
powershell -File docs/audits/managed-distribution/scripts/check_defects_references.ps1
powershell -File docs/audits/managed-distribution/scripts/scan_secrets.ps1
# 7. Audit B 回归（脚本见 scripts/）
powershell -File docs/audits/managed-distribution/scripts/audit_b_recheck.ps1
```

## D. 证据索引
- `evidence/00-git-baselines.txt` — 三仓 git status 基线输出
- `evidence/01-rust-unwrap-expect.txt` — Rust 风险模式全量命中
- `evidence/02-desktop-external-urls.txt` — 桌面外部 URL 清单
- `evidence/03-credential-pattern-masked.txt` — 凭据模式命中（脱敏）
- `evidence/04-sec001-credential-evidence.txt` — IYW-SEC-001 凭据证据（脱敏）
- `evidence/05-worktree-drift.md` — 工作区漂移复核（未提交改动记录）
- `evidence/06-static-audit-notes.md` — 静态审计补充（NSIS/阻塞 IO/跨 await 锁/忽略 Result/SQL 索引/重试超时/基线行号对照）

