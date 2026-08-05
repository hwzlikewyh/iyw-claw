# 静态审计补充证据（Audit A 第二轮补充，2026-08-01）

> 覆盖 Task 12 静态检查清单中上一轮未落证据的类别：
> 桌面升级/安装器覆盖、async 阻塞 IO、跨 await 锁、忽略 Result、SQL 缺索引、重试/超时边界。
> 引用版本说明：标注「基线」= Audit A 基线提交（iyw-claw b46a4c4 / iyw-fusion-api d0df3bc / skill ea76a4e）；
> 标注「工作区」= 2026-08-01 复核时三仓未提交并行任务改动的当前工作区版本（未合并、未验证）。
> fusion-api HEAD 已前进到 7b1c0a8（Task 01 契约与迁移已本地提交，未推送 origin）。

## 1. 桌面安装器与更新覆盖范围（NSIS）

- 文件：`src-tauri/windows/installer-hooks.nsh`（工作区版本；基线无此实现，属 T06 并行改动）。
- 安装根布局：`app` 是唯一会被应用更新替换的区域；`runtime/agents/skills/inventory/staging` 为受管内容
  （版本中心初始化/激活）；`config/data/logs` 为持久区。更新永不清理这些目录。
- 更新替换（`NSIS_HOOK_PREINSTALL`）：先 `GetFullPathName` 规范化，证明目标就是 `$IywClawRoot\app` 才
  `RMDir /r` + 重建；跳过路径直接 `iyw_skip_app_replace`。删除路径有 canonicalize 保护。
- 卸载：默认保留用户数据（config/data/skills/user），仅删除可重建的 `runtime/staging/logs`；
  `/PURGE` 为独立确认动作，删除整个根目录并删注册表键。
- 观察点（未定级）：
  - `taskkill /F /T` 强制结束 `iyw-claw.exe` 与 `iyw-claw-mcp*.exe`，无优雅退出/drain 窗口，
    可能丢失未落盘状态（App 内部有原子写与 fsync 缓解，动态验证需远端 CI）。
  - `/PURGE` 使用 `RMDir /r` 前已对 `$IywClawRoot` 做空串与 GetFullPathName 检查，未发现越界路径。

## 2. 应用更新替换（src-tauri/src/update/install.rs，工作区版本）

- 原子替换链：临时文件写入 → fsync 文件 → `rename` 原子换入 → fsync 目录；
  备份为 hard link + swap + fsync（install.rs:653-746）。
- web 目录先 `std::fs::canonicalize` 再处理（install.rs:122-124）；staging 残留清理（install.rs:316）。
- 正向结论：更新替换以 rename 为基础，具备断电恢复与回滚（`rollback()` + `rollback_available()`）。

## 3. async 阻塞 IO

- `tokio::task::spawn_blocking` 使用 26 处（排除 experts/skills），集中在：
  `web/handlers/upload_jail.rs`、`commands/backup/*`、`pets/marketplace.rs`、`user_memory/service.rs`
  （文件锁）、`acp/version_center/installer/{component,resumable}.rs`、`acp/file_system_runtime.rs`。
- 正向结论：文件/压缩/备份等阻塞操作整体路由到 blocking 池；未发现热路径 async 函数内直接做大文件 IO 的静态证据。

## 4. 跨 await 锁（std::sync::Mutex 在 async 上下文）

- 扫描范围：`src-tauri/src`（排除 experts/skills）。`std::sync::Mutex` 出现在 53 个文件，
  `tokio::sync::Mutex` 出现在 50 个文件。
- 启发式（`std::sync::Mutex` 文件内 `.lock()` 后 12 行内出现 `.await`）候选 164 处，
  该启发式跨函数边界有大量误报。
- 人工抽查（基线已确认热点）：
  - `web/mod.rs`：`Mutex<Option<JoinHandle>>`/`Mutex<Option<Sender>>`/`Mutex<String>` guard 均为
    短生命周期赋值/读取，未持锁跨 await。
  - `terminal/manager.rs`：`Arc<Mutex<HashMap<...>>>` 全部为短临界区（lines 246/344/356/384/429/459），
    未持锁跨 await。
- 结论：未发现已确认的跨 await 持锁；164 处候选需 Audit B 静态分析工具或逐文件人工确认，
  候选清单未入库（生成命令见 verification-matrix C 节）。

## 5. 忽略 Result

- `chat_channel`/`acp`/`user_memory` 范围 `let _ = ` 命中 163 处；`tokio::spawn` 59 处。
- 关键命中（与缺陷关联）：
  - `chat_channel/backends/weixin.rs:742`（工作区）`let _ = WeixinBackend::do_send(...)`——
    出站发送结果被忽略，用户可能只见“没有回复”（IYW-CHANNEL-006 静态佐证）。
  - `chat_channel/command_dispatcher.rs:101`、`command_response.rs:143`、`event_subscriber.rs:380`（工作区）
    `let _ = create_log*`——消息日志写入失败静默（设计为不阻塞主流程，但无告警可观测）。
- 正向：`web/handlers/files.rs` 上传冲突用有界尝试（`UPLOAD_COLLISION_SUFFIX_ATTEMPTS=999` 为文件名去重，
  非重试风暴；`UPLOAD_UUID_FALLBACK_ATTEMPTS=16`）。

## 6. 重试/超时边界

- 有界重试为主：`install.rs:14 PACKAGE_DOWNLOAD_ATTEMPTS=3`、`commands/acp.rs:572 NPM_INSTALL_ATTEMPTS=3`、
  `commands/mcp.rs:1000 MAX_ATTEMPTS=3`、`delegation/broker.rs:1069 CLAIM_POLL_ATTEMPTS=200`。
- `.timeout(` 使用 23 处；`loop {` 无计数循环 55 处（多为事件循环/轮询，需动态验证退出条件）。
- 关注点：`update/version.rs` 注释 “No overall request timeout” 保护慢速下载，取消/断点续传需故障注入验证。

## 7. SQL 索引与参数化

- `scripts/mysql/001_init_schema.sql`（工作区，随 7b1c0a8）：全部业务表均有主键 + 查询键唯一索引
  （skills.slug、skill_versions(skill_id,version)、skill_files(version_id,path)、
  background_jobs(job_type,dedupe_key)、skill_artifacts(version_id,generation)、
  app/agent 发布矩阵 uk_* 等）。031/032/033 已随 7b1c0a8 提交（本地，未推送）。
- 未发现用户输入拼接 SQL：GORM 参数化 Where 为主；`admin.go` 静态 Exec 为配置写入，无输入插值。
- 结论：未见缺索引查询的静态证据；EXPLAIN 与慢查询样本留 Audit B 运行时验证。

## 8. 基线 vs 工作区关键行号对照（供引用时区分）

| 位置 | 基线（b46a4c4） | 工作区（未提交） |
| --- | --- | --- |
| system_skills/git.rs 硬编码凭据 | :48 存在 | 已移除，改 inject_credentials_for_url/require_origin_credentials |
| system_skills/manager.rs dirty 分支 | :93 force_reset | :91-96 tracing::info 后照常 force_reset（2026-08-05 决策，BlockedDirty 已移除） |
| chat_channel/manager.rs | :331-390 启动扫描 + token gate | :379 reconcile_all_enabled；wecom 不再要求 token |
| commands/chat_channel.rs | 无 reconcile | create/update/connect 均走 reconcile_channel |
| src/lib/chat-channel-config.ts | buildChatChannelConfig | + buildChatChannelConfigPatch（:47）已接入编辑对话框 |
| user_memory/context.rs Administrator | :84 | :84 仍存在（未修） |
| user_memory/harvest.rs | 不存在 | 新增（TurnComplete 采集，未验证） |
| update/version.rs GitHub latest.json | :19,24 | :19,24 仍存在（未修） |
| commands/experts.rs include_dir! | 14 个 | 16 个（未修，仍内嵌） |
| install.rs package_size/object_sha256 | :157-196 / :66-71 | :124-234 / :62（未修） |
| tauri.conf.json externalBin/resources | uv/uvx + runtime | 仅 iyw-claw-mcp；resources 仅 out->bundle |
