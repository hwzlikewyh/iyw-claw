# 修复方案 06：Codex SQLite state 安全恢复

## 目标

降低同一 `CODEX_HOME` 并发初始化造成的 SQLite state 启动失败；仅在可证明尚未建立会话、没有任何 Agent 输出且旧进程已经完全回收时自动重试一次。任何数据库移动、重建或清理都必须先备份并由用户确认。

## 已确认现象与边界

`iyw-claw.2026-08-17(4).log` 11:10:45 记录：

```text
Codex process has exited with code 1:
Error: failed to initialize sqlite state runtime under D:\原助理\config\codex:
failed to initialize state runtime at D:\原助理\config\codex
```

连接 `38c6d2b7-420b-42f9-93d3-2e5c1ebab524` 随后终止。

日志没有包含底层 SQLite error code、数据库文件名或 OS error，因此不能确认是：

- 多个 Codex 进程同时初始化造成的锁竞争。
- SQLite/WAL/SHM 损坏。
- 目录权限、磁盘空间、杀毒软件或同步软件占用。
- Codex 自身版本缺陷。

方案必须先增强证据，再做受限恢复，不能直接删除数据库。

## 当前代码已有能力

- managed Codex 统一使用 `CODEX_HOME=<agent-storage>/config/codex`。
- 当前 checkout 的共享 Runtime Host 和按 key singleflight 能减少同一 iyw-claw 进程中的重复启动。
- Runtime Host 启动失败有 shutdown/reap 机制。

仍缺少 canonical profile 的跨进程互斥；当前错误没有专门分类；失败后不会在确认旧进程回收后做一次安全重试；没有用户确认的备份和诊断流程。

## 前置依赖

先实施方案 02，复用其 `ProfileRuntimeKey`、进程生命周期和 failure cleanup。否则 SQLite 修复又会创建一套平行锁，难以保证所有启动入口都遵守。

## 具体改动

### 1. 增加 profile 生命周期跨进程锁

对 canonical `CODEX_HOME` 生成锁文件：

```text
<CODEX_HOME>/.iyw-claw.runtime.lock
```

锁文件内容只写诊断元数据：schema version、iyw-claw PID、app version、启动时间和 profile hash，不写 token 或配置。

行为：

- Runtime Host spawn 前取得 OS exclusive file lock。
- 锁从进程启动一直持有到 Runtime Host 完全 shutdown/reap，不能 Initialize 后提前释放。
- 同一进程的 waiter 复用方案 02 的 singleflight，不重复争抢文件锁。
- 另一个 iyw-claw 进程持锁时等待最多 15 秒，然后返回 `codex_profile_in_use`，带脱敏的 owner PID 和 age。
- owner PID 已不存在时，重新尝试 OS lock；不能只凭锁文件内容判定 stale。
- lock guard 不得跨 await 持有 registry map mutex 或 Agent storage write lock。

这会保证 managed profile 同一时间只有一个 Codex Runtime Host。外部 Codex 若使用不同 `CODEX_HOME` 不受影响。

### 2. 精确分类初始化错误

在 Runtime Host 启动错误映射中增加：

```text
codex_sqlite_state_init_failed
codex_profile_in_use
codex_profile_permission_denied
codex_profile_disk_full
codex_state_unknown
```

优先使用 Codex 结构化错误；没有结构化字段时，只对以下稳定文本组合做严格匹配：

```text
failed to initialize sqlite state runtime
failed to initialize state runtime at
```

保留最多 2048 字符、去换行和敏感片段后的 stderr tail。日志增加 app version、Codex version、profile hash、PID、exit code、是否已创建 session、是否已有 Agent event。

### 3. 一次性自动重试

仅当下列条件全部成立时重试一次：

1. 错误分类为 `codex_sqlite_state_init_failed` 或 `codex_profile_in_use`。
2. `SessionStarted` 尚未发生，`external_id` 为空。
3. `first_agent_event_at` 为空且 output probe 没有输出。
4. 没有 tool、terminal、delegation 或副作用提交。
5. 失败 Runtime Host 已 shutdown，driver 和子进程已 reap。
6. canonical profile lock 已重新取得。
7. 当前调用的 `sqlite_retry_count == 0`。

流程：

```text
classify -> shutdown/reap -> release/reacquire profile lock
-> wait 750 ms -> retry one time -> publish final result to all waiter
```

第二次失败直接返回，不循环，不删除或修改任何 SQLite 文件。所有 singleflight waiter 共享这一次重试，不能每个 waiter 各重试一次。

### 4. 增加只读诊断命令

新增内部诊断输出，不默认展示敏感路径：

- canonical profile hash 和可写性。
- 磁盘剩余空间。
- 当前锁 owner PID 是否存活。
- `*.sqlite`、`*.sqlite-wal`、`*.sqlite-shm` 的文件名、大小和最后修改时间。
- 所有 iyw-claw 管理的 Codex Runtime Host PID。
- 最近一次分类错误和 retry outcome。

只读诊断不得打开数据库写连接，不执行 checkpoint、vacuum 或 migration。

### 5. 用户确认的修复流程

自动重试仍失败时，只提供“诊断”和“备份后重建状态”两个明确动作。重建必须由用户确认，并按以下顺序：

1. 禁止新 Codex 启动。
2. 取消或等待所有 Codex 回合结束；有活动回合时默认终止操作。
3. shutdown/reap 所有 managed Codex Runtime Host。
4. 取得 profile exclusive lock。
5. 检查目标路径仍是 canonical managed `CODEX_HOME`，禁止盘符根目录、用户主目录或 agent-storage 根目录。
6. 将匹配的 DB/WAL/SHM 文件复制到：

```text
<CODEX_HOME>/recovery/<UTC timestamp>/
```

7. 对备份逐文件记录大小和 SHA-256，写入 `manifest.json`。
8. 仅对确认的 SQLite DB 执行只读 `PRAGMA quick_check`；失败结果写入 manifest。
9. 用户再次确认后，将原 DB/WAL/SHM 原子移动到同一 recovery 目录，不删除。
10. 启动一个 Codex Runtime Host，让 Codex 自己创建新 state；成功后保留备份供人工回退。

如果无法确认实际 DB 文件名，不得用 `*.sqlite*` 直接批量移动；必须先从诊断结果中由用户选定文件族。

## 聚焦验证

1. 同 profile 两个应用进程并发启动：只有一个取得锁，另一个返回 profile_in_use，不进入 Codex Initialize。
2. 第一次注入 SQLite init failure、第二次成功：只 spawn 两次，第一次已完全 reap，所有 waiter 共享成功结果。
3. 已有 SessionStarted 或 Agent output 时发生相同文本：不自动重试。
4. 连续两次失败：第二次直接返回，retry count 为 1。
5. permission denied、disk full 和未知错误不误分类为锁竞争。
6. 备份流程在活动回合、无法取得锁、路径越界或备份校验失败时停止，不移动原文件。
7. 重建失败时可以从 recovery manifest 找到并恢复原 DB/WAL/SHM 文件族。

## 验收标准

- managed `CODEX_HOME` 的 Runtime Host 跨进程并发数为 1。
- 自动恢复最多重试一次，且重试前旧 PID 已退出。
- 任何自动路径都不删除、移动、checkpoint、vacuum 或重建 SQLite 文件。
- 日志能区分 profile 占用、SQLite init、权限、磁盘和未知错误。
- 用户修复前一定生成可校验备份，操作目标可追溯并可回滚。

## 发布后验证

在新安装版并发打开多个会话并重复冷启动，按 `profile_hash` 检查：

- 不出现两个 Runtime Host PID 同时持有同一 profile。
- sqlite retry 的首次失败、reap、锁重取和最终结果顺序完整。
- 若仍失败，收集分类后的底层错误，再决定是否需要针对 Codex 版本或文件损坏处理；不能仅凭顶层文本宣布数据库损坏。

## 授权与回滚

- 跨进程锁约定影响所有 Codex 启动入口，实施前需要确认。
- 备份、移动或重建状态文件属于破坏性操作，必须由用户单独确认，不能随代码修复自动执行。
- 回滚代码时可以关闭自动 retry，但应保留错误分类和 profile lock；恢复数据库时只根据 recovery manifest 操作，不使用通配符删除。
