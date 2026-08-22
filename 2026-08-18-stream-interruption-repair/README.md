# iyw-claw 断流问题修复方案索引

## 结论

三份日志中的“断流”不是一个单点故障，而是六类问题叠加。当前 checkout 已有 prompt 静默 watchdog、Codex 共享 Runtime Host、分层启动超时、空闲连接回收等保护，但只能算部分修复；应用更新安装门禁、Artifact 查询合并、内存压力迟滞、Codex profile 跨进程锁和 SQLite 定向恢复仍未闭环。

本目录把每个修复点拆成独立方案，任何一份都可以单独排期、实现、验收和回滚。

## 证据范围

分析基于以下日志：

- `iyw-claw.2026-08-17(4).log`
- `iyw-claw.2026-08-18.log`
- `iyw-claw.2026-08-18(1).log`

关键事实：

| 修复点 | 日志证据 | 当前源码状态 | 结论 |
| --- | --- | --- | --- |
| ACP prompt 静默 | 共出现 11 次约 600 秒无 Agent 事件；同一连接在 27 分钟内被 watchdog 取消两次 | 已有静默检测、取消和部分 prompt 生命周期日志 | 部分修复，缺取消超时后的强制隔离及全链路阶段证据 |
| Codex 启动并发 | 2026-08-18 00:55:25 至 00:55:29 内启动 3 个 Codex 连接并分别 Initialize | 当前 checkout 已有 Runtime Host 按 key 单飞和 120/125/150 秒超时 | 当前源码较日志版本有改进，但 profile 级边界、失败冷却和后台升级互斥仍需补齐 |
| 更新中断活动回合 | 00:55:30 prompt 开始，01:01:27 NSIS 安装器启动并退出桌面进程，中间没有该连接的 prompt 完成记录 | `install_update()` 调用前没有活动回合二次检查 | 未修复 |
| Artifact 查询风暴 | 会话 4 共 2276 次查询；峰值 161 次/秒；请求中位间隔约 6.25 ms | 每个 hook 实例独立请求、订阅和 debounce；`filters` 对象会使 load/effect 失稳 | 未修复，且会放大日志、SQLite 和内存压力 |
| 内存压力治理 | 8 GB 机器 19 次进入 emergency；可用内存范围约 483 至 2207 MiB | 已保护活动回合，只回收可恢复的空闲连接 | 部分修复；阈值对 8 GB 机器过高，无迟滞，也未统一阻止后台重任务 |
| Codex SQLite state | Codex 在 `D:\原助理\config\codex` 初始化 SQLite state runtime 失败并退出 | 共享 Runtime Host 能减少同进程重复启动，但没有 profile 跨进程锁和该错误的一次性安全重试 | 未闭环；日志不足以确认是锁竞争、损坏、权限还是磁盘问题 |

## 独立方案

1. [ACP prompt 静默恢复](./01-acp-prompt-silence-recovery.md)
2. [Codex 启动并发治理](./02-codex-startup-concurrency.md)
3. [更新安装活动回合门禁](./03-updater-active-turn-guard.md)
4. [Task Artifact 查询风暴治理](./04-task-artifact-query-storm.md)
5. [内存压力治理](./05-memory-pressure-governance.md)
6. [Codex SQLite state 安全恢复](./06-codex-sqlite-state-recovery.md)

## 建议实施顺序

1. 先实施方案 04，立即消除每秒上百次查询造成的放大效应。
2. 实施方案 03，阻止桌面更新直接终止正在生成的回合。
3. 实施方案 02，统一 Codex 启动入口和并发边界。
4. 在方案 02 的 profile 生命周期基础上实施方案 06。
5. 实施方案 01，补齐 prompt 全链路观测和取消失败后的隔离。
6. 最后实施方案 05，并在查询风暴消失后重新采集内存基线再校准阈值。

方案 04 和 03 可以并行开发。方案 06 依赖方案 02 的 canonical profile key 和生命周期锁。方案 05 的阈值验收应在方案 04 上线后进行，避免把查询风暴造成的负载误判为 Agent 固有内存占用。

## 统一发布验证

这些方案针对当前源码设计，不代表日志对应的 `0.1.81/0.1.82` 安装版已经包含修复。每个方案完成后都要分开验证：

1. 静态源码和定向检查通过。
2. 生成新的桌面安装包。
3. 在 8 GB Windows 机器安装该版本。
4. 用版本号、进程 PID、结构化日志确认运行的是新包。
5. 按各方案的运行时验收步骤复测，不以源码存在或构建成功代替安装版验证。

## 授权边界

以下内容不在本轮文档任务中执行，进入实施前需要用户确认：

- 修改共享 update state、前后端 contract 或公共类型。
- 引入新的跨进程文件锁约定。
- 新增依赖或修改根配置、CI、数据库 schema。
- 停止所有 Codex 进程、备份、移动或重建 Codex SQLite state 文件。
- 提交、推送或发布桌面安装包。
