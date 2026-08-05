# 可靠性审计报告（reliability.md）

> Audit A 只读基线 · 2026-08-01 · 结论先行：Audit A 确认 2 个确定性可靠性 P0（IYW-SKILL-001 长度语义、IYW-SKILL-002 破坏性 reset）、1 个 P0 完整性（IYW-SKILL-008）与 3 个 P1 断链族（IYW-SKILL-006 状态机、IYW-JOB-001 ticker、IYW-MEMORY-001 采集闭环）；消息渠道 6 项 P0/P1 全部确认（IYW-CHANNEL-001..006）。
> 工作区漂移（2026-08-01 复核）：下述「工作区」状态均未提交、未验证；缺陷状态与 worktree_fix 见 defects.yaml。

## 1. 确定性故障（P0）

### IYW-SKILL-001（P0，confirmed）
- 现象：`expected_size=19644, received_size=14339` 重试必失败；`install.rs` 用 `package_size`（raw size）校验动态 ZIP 字节数。
- 根因：`direct_upload_init.go:159` 把原始文件大小总和写入 `Version.PackageSize`；`download.go:109-135` 在 `StorageKey==""` 时运行时 Deflate 生成 ZIP 且 `Size:-1`。长度语义确定性不匹配，不是网络截断。
- 关闭前置：Task 01 冻结 artifact 契约 → Task 03 构建确定性 ZIP 上传 TOS、冻结 size/sha → 客户端只比较 artifact size。

### IYW-SKILL-008（P0，confirmed，T03 台账重编号）
- 现象：即使去掉长度校验，`objectSha256` 校验仍失败——动态 ZIP 响应与空/错摘要不是同一字节序列（`install.rs:66-71`）。
- 关闭前置：冻结 `artifact_sha256`（真实 ZIP 字节摘要），客户端按 artifact 摘要校验；不得拿 content digest 冒充 object digest（IYW-SKILL-007 语义区分）。

### IYW-SKILL-002（P0，confirmed；已按产品决策接受）
- 现象（基线）：`manager.rs:93` 检测到 dirty checkout 后 `git::force_reset`（`git.rs:30-35` = `reset --hard HEAD`），破坏用户修改。
- 2026-08-05 决策：系统 Skill 以远端为唯一事实来源，dirty checkout 一律强制覆盖。`BlockedDirty` 门禁与启动自动更新开关已移除；dirty 分支只记录 `tracing::info` 后照常 `force_reset`，启动更新恒定执行。
- 关闭前置：回归验证 dirty fixture 更新后远端内容覆盖成功、UI 常驻提示可见、无凭据时返回配置错误而非反复重试；未合并前不视为 verified。
## 2. 后端并发与持久化（P1）

### IYW-JOB-001（P1，confirmed）
- 现象：后台任务只有进程内 ticker（`bootstrap/agent_release.go:75-81`、`skill_upload_cleanup.go:23-29`），无持久表/租约/重试/死信；031 迁移已随 fusion-api 7b1c0a8 本地提交（未推送），jobcenter 实现未合并。
- 影响：镜像/构建任务在实例重启后丢失；多实例会重复执行；无 claim/fencing。
- 关闭前置：Task 02 按 031 实现 jobcenter（persistent claim、retry、cancel 事件、死信），Task 13 接线。

### IYW-SKILL-006（P1，confirmed）
- 现象：`CompleteUpload` 直接置 ready（`direct_upload.go:143-154`），无冻结 artifact 即进入可安装列表，产生“永远下载失败”的版本。
- 关闭前置：版本状态机增加 `artifact_pending`；build 成功/失败都要有明确状态，失败不入可安装列表。

## 3. 消息渠道（IYW-CHANNEL-001..006，全部 confirmed）
| ID | 严重度 | 现象 | 根因/证据（基线） | 工作区（未提交、未验证） |
| --- | --- | --- | --- | --- |
| 001 | P0 | 新建 enabled 渠道不连接；仅启动自动连接一次 | `manager.rs:331-390` 只有启动扫描；创建对话框无 connect | `commands/chat_channel.rs:62-65` 创建后 reconcile；manager.rs:379 `reconcile_all_enabled` |
| 002 | P0 | disabled→enabled 不连接；enabled→disabled 仅已连接时断开 | toggle 无 reconcile | `commands/chat_channel.rs:233-235` 每次更新后 reconcile |
| 003 | P0 | 企微在 connect/test/auto-connect 前要求 keyring token | `manager.rs:367-379` token gate 前置 | `backends/mod.rs` 企微不再要求 token（凭据存 wecom-cli） |
| 004 | P1 | 编辑重建整段 JSON，覆盖 `channel_workspace_root` 等 | `edit-chat-channel-dialog.tsx:81-96`；`config_patch.rs` 未接线 | `chat-channel-config.ts:47` `buildChatChannelConfigPatch` 已接入编辑对话框 |
| 005 | P1 | 测试连接只验凭据，无端到端回环 | `backends/wecom.rs:239` | `test_chat_channel_core` 增加 credential_ready 前置；仍无回环 |
| 006 | P1 | 无统一 readiness 呈现；出站发送结果部分被忽略 | `readiness.rs` 未接入 UI | ReconcileOutcome 上抛“已保存，连接失败”；weixin.rs:742 出站结果仍被忽略 |

- 共同根因（基线）：连接生命周期只有“启动一次”，无统一 reconcile/connect 语义；配置更新走全量重建而非 merge patch。
- 关闭前置：工作区修复合并后做三渠道回环、disabled/enabled 幂等、编辑字段保留、失败上抛的回归验证（远端环境）。
## 4. 用户记忆（IYW-MEMORY-001..003）
- MEMORY-001（P1）：只有模型主动调用 append/propose，无 TurnComplete 可靠采集队列（基线 `candidate_lifecycle.rs` 有候选/去重/确认，但无采集任务）。复核时（工作区）新增 `user_memory/harvest.rs`（TurnComplete 采集），候选上限/风险过滤/确认闭环未验证。
- MEMORY-002（P0）：`context.rs:84` 提示模型写 `C:/Users/Administrator/.iyw-claw/user-memory.md` 硬编码路径，与实际 resolved root 不一致；MCP 失败时不应指示模型写文件。
- MEMORY-003（P1）：设置页载入/保存对 content 做 sanitize（`user-memory-settings.tsx:29,78（工作区）`），可能清除 `<!-- entry_id -->` marker，破坏去重与纠正定位；展示内容与持久原文未分离。

## 5. 配置与升级（IYW-CONFIG-001 / IYW-DIST-001）
- CONFIG-001（P1）：基线 fingerprint 基础设施存在（`acp.rs:6841-6914`），但强制 gate 未闭环。复核时（工作区）新增 `acp/session_config_reconciler/`（幂等重写+回读校验），接线与强制 gate 未验证。
- DIST-001（P1）：基线包内携带运行时/Skill + 直连多上游；复核时（工作区）安装包已瘦身（externalBin 仅 mcp、resources 仅 bundle）、installer-hooks.nsh 改为 app 区替换+持久区布局，但直连上游仍在；应用更新清单直连 GitHub（DIST-002，P2，未修）。桌面升级删除/覆盖范围需远端 CI 验证（更新只替换 app 区，不得动 runtime/config/data/logs/skills/agents/本地库存/用户设置/记忆）。
## 6. 正向证据（已检查）
- 下载重试有界：`install.rs` `PACKAGE_DOWNLOAD_ATTEMPTS` 有限次；`lifecycle.rs` `HANDLE_EVENT_RETRY_BACKOFFS` 有限退避。
- ZIP 解包有完整防御（`skill_package.rs`：数量/大小/路径/符号链接/加密条目限制）。
- Fusion 外部调用有超时与连接池；记录用有界队列，正常退出给 drain 窗口（AGENT.md 约定，需 Audit B 运行时验证）。
- 前端列表有请求序号防旧响应覆盖（`skill-market-data-list.tsx`）。

## 7. 动态验证缺口（Audit B / 远端 CI）
- 下载故障注入（对应 0801README 质量门禁）：断网、URL 过期、TOS 403/404/429/5xx、慢流、截断、Range 不支持、摘要错、签名错、磁盘满、进程退出、重复启动、并发计划、并发同版本下载。
- 后端中断：MySQL/TOS/Git 中断、worker crash、双实例 claim/fencing、租约过期、shutdown drain、queue 上限与 backpressure。
- 桌面文件系统：Windows 更新占用、被占用文件（防病毒延迟）、路径含 Unicode/空格、junction/symlink/reparse point、路径大小写、磁盘满、原子 rename 中断恢复、LKG 回滚。
- 渠道：三渠道真实账号回环（入站→dispatcher→workspace→resolve→spawn→prompt→TurnComplete→出站→回执）、消息/附件大小、富文本降级、重启恢复、echo suppression。
- 记忆：TurnComplete 采集上限、敏感推断过滤、迟到写、事务恢复、marker 保留。
- 会话：新会话 fingerprint gate、恢复旧会话不混代际、配置并发写、MCP 工具暴露与 bridge readiness 一致。
- 离线：离线首次安装明确阻断；已初始化离线重启可用。
- 前端：Playwright 1280x800 / 1440x900 / 1920x1080 / 窄屏，无重叠、无横向溢出、键盘焦点、错误状态、大列表滚动性能。
- 当前机器禁止桌面编译/启动，以上全部需要远端 CI/测试机证据。
