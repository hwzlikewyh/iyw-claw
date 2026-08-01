# 验证矩阵（verification-matrix.md）

> Audit A 只读基线 · 2026-08-01 · 每条证据必须能回到真实命令/文件/源码位置。
> 桌面动态证据一律来自远端 CI/测试机；当前机器未编译、未启动任何桌面产物。

## A. 已执行静态验证（Audit A）

| 类别 | 命令/方式 | 结果 | 证据 |
| --- | --- | --- | --- |
| 三仓基线 | `git status --porcelain -b` + `git log --oneline -5` | iyw-claw b46a4c4（master）；fusion-api d0df3bc；skill ea76a4e；均有未提交任务外改动（不动） | 见本目录 evidence/00-*（下方汇总） |
| Secret 扫描 | rg 凭据模式（已脱敏输出） | 命中 IYW-SEC-001（git.rs:48）；无其他源码命中；历史同源 commit 3b6e21b | `evidence/03-credential-pattern-masked.txt`；security.md §1 |
| 外部 URL 清单 | `rg -o 'https?://...'` | 桌面 50+ 直连上游 URL 已登记 | `evidence/02-desktop-external-urls.txt`；inventory.md §3 |
| Rust 风险模式 | `rg '\.unwrap\(\)|\.expect\(|panic!|unreachable!'` | 118 处；抽查 web/mod.rs（锁）、remote_image（长度守卫）、terminal/manager.rs（锁）未见用户输入直触 panic | `evidence/01-rust-unwrap-expect.txt` |
| include 内嵌 | `rg 'include_dir!|include_bytes!'` | experts.rs 14 个 Skill 内嵌（IYW-SKILL-003）；tray icon include_bytes | reliability/security |
| SQL 注入面 | `rg 'fmt.Sprintf|\.Raw\(|\.Exec\(|\.Query\('` | 未发现把用户输入拼 SQL；GORM 参数化 Where 为主；admin.go:131 有静态 Exec（改配置） | 扫描记录（fusion-api） |
| 日志脱敏 | 读 relay http_utils.go / record_body.go / upstream_logging.go | preview 512B 上限 + RedactedPreview + imageErrorPreview；测试断言不泄漏 | security.md §1.2 |
| 前端 Snowflake | `rg 'parseInt\(|Number\('` | 命中点均为本地小整数（folderId/attempt/zoom）；Skill 市场用 string id | security.md §2.2 |
| 前端竞态 | 读 skill-market-data-list.tsx | 有 requestId 防旧响应覆盖 + 防抖 + 分页（正向） | reliability.md §6 |
| 管理页鉴权 | 读 admin/admin.go | `/admin/api` 有 adminAuth；`/admin/*` 静态页无鉴权（P3，IYW-AUTH-002） | security.md §2.1 |
| 渠道断链 | 读 manager.rs / edit-chat-channel-dialog.tsx | IYW-CHANNEL-001..006 全部确认 | reliability.md §3 |
| 记忆断链 | 读 context.rs / user-memory-settings.tsx | IYW-MEMORY-001..003 全部确认 | reliability.md §4 |
| 系统 Skill | 读 git.rs / manager.rs | IYW-SEC-001、IYW-SKILL-002 确认 | defects.yaml |
| skill 发布源 | 读 experts.toml + `git tag` | bundle.version=0.0.11 vs 标签最高 v0.0.8（IYW-SKILL-013） | defects.yaml |

## B. 未执行 / 无法在本机执行的验证（原因与责任人）

| 类别 | 原因 | 责任人 | 目标 |
| --- | --- | --- | --- |
| 桌面编译/启动/打包/E2E | 本机禁止（0801README 硬规则） | 远端 CI/发布机 | Audit B |
| 渠道真实账号回环 | 需要三渠道账号与运行环境 | T08 + 远端 | Audit B |
| TOS/MySQL/Git 故障注入 | 需要隔离运行环境 | T12（脚本）+ T13 | Audit B |
| Fusion unit/contract/race/benchmark | 本机可运行，但 Wave 0 未冻结、代码未落盘，当前无新增代码可测；存量测试可在隔离环境跑 | T01 冻结后 | Audit B |
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
```

## D. 证据索引
- `evidence/01-rust-unwrap-expect.txt` — Rust 风险模式全量命中
- `evidence/02-desktop-external-urls.txt` — 桌面外部 URL 清单
- `evidence/03-credential-pattern-masked.txt` — 凭据模式命中（脱敏）
- 三仓 `git status` 基线输出已存档于本矩阵；具体 status 输出见本目录 `evidence/00-git-baselines.txt`
