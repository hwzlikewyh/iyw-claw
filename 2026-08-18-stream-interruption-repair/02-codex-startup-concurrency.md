# 修复方案 02：Codex 启动并发治理

## 目标

同一个 canonical `CODEX_HOME`、运行版本和进程配置在任一时刻只允许一个 Runtime Host 启动。并发会话共享启动结果；失败必须完整清理并在短暂冷却期内复用同一错误，避免启动风暴。

本方案不改变多会话能力，只收敛承载这些会话的 Codex 进程启动边界。

## 已确认现象

`iyw-claw.2026-08-18.log` 在四秒内出现三组 Codex 启动：

- 00:55:25：连接 `2403c251-...` 开始 Initialize。
- 00:55:28：连接 `a71b73c5-...` 开始 Initialize。
- 00:55:28：连接 `ec53f8a0-...` 再次开始 Initialize。

这说明日志对应的安装版会为相近时间到达的会话分别启动 Codex。该行为会放大 CPU、内存和 SQLite profile 竞争。

## 当前代码已有能力

当前 checkout 已引入：

- `RuntimeHostRegistry::spawn_host` 按 `RuntimeHostKey` 使用 `spawn_locks` singleflight。
- Codex 默认启用共享 Runtime Host。
- Initialize 120 秒、Host ready 125 秒、spawn handshake 150 秒三层超时。
- `HostStartupGuard`、`HostStartups` 和 shutdown/reap 清理。
- Agent 自动升级会检查全局空闲和 Agent storage work。

这比日志版本明显前进，但仍有四个缺口：key 没有显式暴露 canonical profile 身份；`start_owned`/fallback 路径可绕过共享 host；失败后没有统一冷却；自动升级、prewarm、前台启动之间没有同一个 profile 生命周期门禁。当前源码也尚未通过新安装包运行验证。

## 具体改动

### 1. 定义唯一的 ProfileRuntimeKey

在 `agent_profile.rs` 增加 canonical profile 解析，输出：

```rust
struct ProfileRuntimeKey {
    agent_type: AgentType,
    canonical_profile_root: PathBuf,
    runtime_version: String,
    process_fingerprint: String,
    policy_revision: Option<u64>,
}
```

Windows 规范化要求：

- 必须是绝对路径。
- 路径存在时使用 `canonicalize()`；不存在时规范化父目录后再拼接末级。
- 盘符统一大写，比较时使用 Windows 大小写不敏感语义。
- 去除尾部分隔符，但不能解析成空路径或盘符根目录。

将该 key 作为 `RuntimeHostKey` 的组成部分，并在日志中只记录 profile hash，不记录完整用户路径。

### 2. 统一所有 Codex 启动入口

以下入口全部调用 `RuntimeHostRegistry::acquire_profile_runtime`：

- 正常新会话和恢复会话。
- `prewarm_codex_runtime`。
- HTTP MCP fallback 后的 owned host。
- probe、delegation 和其他后台会话。

`start_owned` 不能绕过 profile singleflight。确实需要独占 host 时，仍先取得同一 profile 的启动 lease，再用不同 host mode 标识；同一 profile 不允许两个 Initialize 并行。

### 3. 明确启动状态机

每个 key 保存以下状态：

```text
Absent
  -> Starting(started_at, waiter_count)
  -> Ready(host)
  -> Failed(error_class, failed_at, cooldown_until)
```

规则：

- 第一个调用者负责启动，后续调用者只等待同一个结果。
- 启动成功后所有 waiter 获得 route reservation。
- 启动失败后先 shutdown/reap 子进程，再发布失败结果。
- 失败结果缓存 3 秒；冷却期内不重新 spawn，直接返回同一分类错误。
- 冷却结束后只有一个调用者能从 Failed 转回 Starting。
- shutdown 开始后拒绝新启动，并唤醒所有 waiter 返回 `registry_shutting_down`。

### 4. 统一超时预算

保留现有数值，但改成一个递减预算，避免三层独立计时互相覆盖：

| 阶段 | 上限 | 超时动作 |
| --- | ---: | --- |
| profile/env/prompt preparation | 15 秒 | 返回 `startup_prepare_timeout`，不得 spawn |
| 进程 spawn 和 stdio 建立 | 15 秒 | kill/reap 子进程 |
| ACP Initialize | 120 秒 | 发送 shutdown，5 秒后 abort/reap |
| SessionStarted 等待 | 使用 150 秒总预算的剩余时间 | 移除未完成 connection，并复用已分类错误 |

总预算从进入 singleflight 时开始。等待已有 Starting 的调用者记录 `shared_wait_ms`，但不获得一套新的 150 秒预算。

### 5. 后台升级与会话启动互斥

在现有 `agent_storage_work` 基础上增加按 profile 的生命周期门禁：

- Runtime Host 启动和运行持有 read lease。
- 激活新 Agent 版本、迁移 profile、切换 profile root 持有 write lease。
- 自动升级允许后台下载，但激活只能在没有 Starting/Ready host 且全局空闲时执行。
- prewarm 在升级 write lease 等待或内存压力非 comfortable 时直接 defer。
- write lease 等待不得占用 Runtime Host registry 的内部 map lock。

## 结构化日志

每次 acquire 只记录一条结果：

```text
agent_type, profile_hash, runtime_version, fingerprint_prefix,
outcome=hit|waited|spawned|failed|cooldown,
waiter_count, wait_ms, stage, elapsed_ms, pid, error_class
```

启动阶段日志只在状态转换时产生，不能在等待循环中重复打印。

## 聚焦验证

1. 50 个并发 acquire 使用同一 key：只执行一次 spawn/Initialize，50 个调用全部获得同一 host。
2. key 的 profile root 或 runtime version 不同：分别启动，不错误共享。
3. 启动失败：所有 waiter 得到同一错误，子进程已 reap，3 秒内不再 spawn。
4. Initialize 超时：进程在 5 秒清理预算内退出，registry 不残留 Starting。
5. 自动升级等待 write lease 时，前台会话不被它持有的其他锁阻塞。
6. prewarm 与前台首会话并发：前台复用同一启动，不产生第二个 Codex 进程。

## 验收标准

- 同一 `ProfileRuntimeKey` 的 Initialize 并发数恒为 1。
- 启动失败后没有孤儿进程、悬挂 waiter 或永久占用的 spawn lock。
- 运行日志中同一 profile 的 `spawned` 与 PID 一一对应。
- 自动升级不再在 Agent 活动恢复后反复开始再失败。
- 新安装版在并发打开/恢复 10 个会话时，Codex Runtime Host 数符合 key 数而不是会话数。

## 发布后验证

在 8 GB Windows 机器执行冷启动、10 会话同时恢复、prewarm 与会话竞争、自动升级等待四组场景。按 `profile_hash + runtime_version` 聚合日志，确认任何时间窗口都没有两个 `Starting`，并用系统进程树核对 PID。

## 风险、授权与回滚

- profile key 属于共享运行时 contract，实施前需要确认。
- 生命周期门禁会影响 Agent 版本激活和 profile 迁移，必须做静态锁顺序审查，禁止在 registry map lock 内等待 write lease。
- 回滚可关闭共享 host feature flag，保留 canonical profile 日志和失败清理；但关闭后 SQLite 并发风险会上升，需同时关闭 prewarm 和自动激活。
