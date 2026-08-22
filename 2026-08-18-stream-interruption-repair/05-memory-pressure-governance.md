# 修复方案 05：内存压力治理

## 目标

让 8 GB Windows 机器在真实低内存时主动回收可恢复的空闲 Agent，同时避免阈值过高造成长期 emergency、反复启停和额外抖动。活动回合、交互等待和工具执行不得被资源治理直接终止。

## 已确认现象

三份日志中共出现 19 次 `resource governor disconnecting idle connection`：

- 物理内存约 7943 MiB。
- 日志中的 shrinking 阈值约 3972 MiB。
- emergency 阈值约 2979 MiB。
- 触发回收时可用内存约 483 至 2207 MiB，全部被分类为 emergency。

当前公式会让普通 8 GB Windows 工作集长期处于 emergency。483 MiB 时确实危险，但 2.2 GiB 可用仍按 emergency 处理过于激进。

同时，Artifact 查询风暴最高 161 次/秒，是独立负载问题。必须先实施方案 04，再评价 Agent 自身内存；不能通过更激进地杀 Agent 来掩盖查询循环。

## 当前代码已有能力

- `resource_governor.rs` 能采集系统内存和 Agent 进程树 private memory。
- `reclaim_block_reason` 会保护 turn、permission、question、confirmation、工具、delegation、terminal、后台任务和可见会话。
- `sweep_excess_idle` 每个 tick 最多回收一个可恢复空闲连接。
- normal 状态尊重用户的空闲 Agent 数量设置；memory pressure 可以覆盖该数量。
- emergency 下允许跳过 recent activity grace，但仍要求连接可恢复且无活动工作。

主要缺口是分类无状态、无迟滞，阈值不适合 8 GB 机器；prewarm、probe、自动升级和非必要新 Agent 没有统一 admission gate。

## 具体改动

### 1. 改为适合桌面的阈值

第一版使用以下进入阈值，并将常量集中在 `resource_governor.rs`：

```text
shrinking_enter = clamp(total * 20%, 1536 MiB, 4096 MiB)
emergency_enter = clamp(total * 10%,  768 MiB, 2048 MiB)
hard_emergency  = 512 MiB
```

对约 8 GB 机器，结果约为：

```text
shrinking_enter ~= 1589 MiB
emergency_enter ~= 794 MiB
hard_emergency  = 512 MiB
```

退出阈值增加 512 MiB：

```text
shrinking_exit = shrinking_enter + 512 MiB
emergency_exit = emergency_enter + 512 MiB
```

这些是首版生产阈值，不是永久常量。上线后按系统 available memory、commit charge、Agent private memory 和回收收益重新校准。

### 2. 增加状态和迟滞

新增 `MemoryPressureTracker`，由 idle sweep task 持有，不再每次采样独立分类：

```text
Comfortable -> Shrinking -> Emergency
```

转换规则：

- 低于 hard emergency：立即进入 Emergency。
- 其他降级：连续 2 个 30 秒样本低于进入阈值后生效。
- Emergency 回到 Shrinking：连续 3 个样本高于 `emergency_exit`。
- Shrinking 回到 Comfortable：连续 3 个样本高于 `shrinking_exit`。
- Unknown 不触发回收，也不清除上一可靠状态；连续 5 次 Unknown 记录一次 WARN。

只在状态转换时记录 INFO，避免每 30 秒重复日志。

### 3. 建立统一后台工作 admission gate

新增只读快照接口：

```rust
ResourceAdmission::decision(work_kind, launch_origin)
```

策略：

| 工作类型 | Comfortable | Shrinking | Emergency |
| --- | --- | --- | --- |
| 用户显式打开/恢复根会话 | 允许 | 先回收一个安全空闲会话后允许 | 若低于 hard emergency 且无可回收对象，返回 `resource_exhausted`；否则允许 |
| prewarm、probe | 允许 | 延迟 | 禁止 |
| Agent 自动升级/后台激活 | 允许 | 只允许下载，禁止激活 | 禁止下载和激活 |
| 自动/推测性 delegation | 允许 | 限制为 1 | 禁止新建 |
| 已经运行的回合 | 保护 | 保护 | 保护 |

接入点：

- `ConnectionManager::prewarm_codex_runtime`
- 桌面和 server startup prewarm 调用处
- `acp::auto_update`
- probe/delegation spawn 入口
- `spawn_agent_with_origin_traced`

前台根会话优先级高于后台任务。后台任务等待 admission 时不得占用 Agent storage write lock 或 Runtime Host spawn lock。

### 4. 明确回收顺序

在现有安全条件基础上，候选排序调整为：

1. 最久未使用。
2. private memory 更大。
3. prewarm host 优先于用户恢复会话。
4. 无可恢复 external session 的连接仍不可回收。

每个 30 秒 tick 最多回收一个。回收后下一 tick 重新采样，不在一次扫描中连续杀多个进程。

### 5. 增加回收收益观测

每次回收记录 before snapshot；30 秒后记录一次 outcome：

```text
pressure_before, available_before, available_after,
agent_private_bytes, connection_id, reclaim_source,
protected_count, candidate_count, admission_blocks
```

如果可用内存没有提升，不重复回收同类连接，并记录 `reclaim_no_gain` 供后续定位非 Agent 内存来源。

## 聚焦验证

1. 8 GB 输入下 2.2 GiB 可用不再是 Emergency；700 MiB 可用进入 Emergency。
2. 在阈值附近上下波动不会每个 tick 反复切换状态。
3. hard emergency 立即生效，不等待两个样本。
4. Prompting、permission、question、tool、delegation 和 terminal 连接永远不进入回收候选。
5. Shrinking/Emergency 下 prewarm 和自动激活不会启动新 Agent 进程。
6. 用户显式根会话在非 hard emergency 时仍能启动，且先尝试安全回收空闲连接。
7. 一次 tick 最多回收一个连接，30 秒后才决定是否继续。

## 验收标准

- 8 GB 测试机在常规 1.6 GiB 以上可用内存时不会长期显示 Emergency。
- 压力状态退出需要稳定恢复，日志中没有 30 秒级别的状态来回切换。
- 资源治理造成的活动回合终止数为 0。
- Emergency 期间 prewarm、probe 和自动升级新进程数为 0。
- 每次回收都能通过结构化日志说明候选、保护原因和实际收益。

## 发布后验证

按新安装版连续采集 48 小时：

- 记录压力状态占比、每次停留时间和转换次数。
- 记录回收前后 available memory 变化及 Agent private memory。
- 将 Artifact 查询风暴修复前后的基线分开比较。
- 若 Emergency 仍高频但 Agent 回收无收益，转向 WebView、Office preview 或其他进程定位，不继续提高回收强度。

## 风险与回滚

- 阈值降低可能推迟回收，因此保留 hard emergency 512 MiB 立即保护。
- 回滚可恢复旧阈值，但应保留 tracker、迟滞、活动回合保护和后台 admission 日志。
- 调整共享性能状态或前端展示 contract 前需要用户确认；首版可只在后端使用新状态，避免扩大改动面。
