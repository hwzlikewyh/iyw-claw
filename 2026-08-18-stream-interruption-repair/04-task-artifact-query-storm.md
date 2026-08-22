# 修复方案 04：Task Artifact 查询风暴治理

## 目标

同一个 Artifact 查询 key 在多个组件、重渲染和事件突发下只执行一条共享请求。消除无变化情况下持续访问 SQLite、扫描文件状态和打印成功日志的循环。

## 已确认现象

对 `iyw-claw.2026-08-17(4).log` 的结构化统计：

- `conversation_id=4` 共 2276 次 `[task-artifacts] list completed`。
- 查询窗口为 09:12:00 至 11:11:31。
- 请求间隔中位数约 6.25 ms。
- 单秒峰值 161 次，多次出现 140 次/秒以上。
- 大多数查询只耗时 0 至 6 ms，这不是单条慢查询，而是调用数量失控。

## 根因

`src/components/layout/use-task-artifacts.ts` 存在两层放大：

1. 调用方每次 render 创建新的 `filters` 对象；`useArtifactLoader` 的 `load` 依赖整个对象，因此 render 后 `load` 身份变化。
2. `useInitialArtifactLoad` 依赖 `load`；请求设置 loading/state 后再次 render，effect 重新执行并发起下一次请求，形成自激循环。
3. 每个 hook 实例独立订阅 `task-artifact://changed`、维护 timer 和 in-flight 请求，无法跨组件合并。
4. 后端每次列表查询还会逐项检查本地文件状态，调用风暴会放大 SQLite、文件系统、日志和内存负载。

## 当前代码状态

- `CurrentReplyArtifacts` 和 `AuxPanelArtifactsTab` 都使用 `useTaskArtifacts`。
- hook 只有实例级 request id 和 80 ms debounce，没有共享 cache/in-flight/subscription。
- `list_task_artifacts_core` 对每次成功查询都打印 INFO。
- 后端没有同 key singleflight 或短期结果合并。

该问题当前没有修复。

## 具体改动

### 1. 先切断 render 自激循环

`useArtifactLoader`、`fetchTaskArtifacts` 和 effect 依赖改用 primitive：

```text
scope, conversationId, folderId, filterKey
```

禁止把调用方创建的 `filters` 对象放进 `useCallback/useEffect` 依赖。即使共享 store 尚未完成，这一步也必须先提交并验证，作为最小止血。

### 2. 新建共享 query store

新增 `src/components/layout/task-artifact-query-store.ts`，模块级维护：

```ts
type QueryEntry = {
  snapshot: TaskArtifactState
  inFlight: Promise<void> | null
  listeners: Set<() => void>
  refreshTimer: ReturnType<typeof setTimeout> | null
  lastLoadedAt: number
  generation: number
}

const entries = new Map<string, QueryEntry>()
```

key 规则：

```text
current:<conversationId>
all:<folderId>
```

行为：

- 同 key 已有 `inFlight` 时，所有调用方复用同一个 Promise。
- 首个 subscriber 建立全局 transport event subscription；最后一个 subscriber 离开后再解除。
- React hook 使用 `useSyncExternalStore` 订阅 snapshot，组件之间不复制网络状态。
- mount 时 1 秒内已有成功 snapshot 直接复用；显式 refresh 和 changed event 不受 stale time 阻挡。
- 没有 subscriber 的 entry 在 60 秒后淘汰，timer 和 Promise 完成后才能删除。

### 3. 合并 changed/reconnect 事件

全局只保留一套订阅：

- `task-artifact://changed` 带 `conversation_id` 时，只刷新 `current:<id>`。
- 现有事件没有 folder_id，因此所有活跃 `all:*` key 各自 debounce 一次；不要为每个组件各刷一次。
- 每个 key 使用 100 ms trailing debounce；timer 存在时只更新 dirty 标记。
- transport reconnect 对所有活跃 key 各触发一次后台 refresh。
- changed 发生在请求进行中时，设置 `refreshAfterFlight=true`，当前请求完成后最多再补一次，不能并发叠加。

若要在事件中增加 `folder_id`，属于共享 contract 修改，需要单独确认；本方案第一版不依赖该改动。

### 4. 增加后端第二道合并保护

在 `src-tauri/src/commands/task_artifacts.rs` 增加 `ArtifactQueryCoordinator`，key 为规范化后的 conversation/folder filter：

- 同 key 并发请求共享一次 `list_artifacts` 结果。
- 成功结果缓存 250 ms，用于吸收跨窗口或旧前端的短突发。
- `register_task_artifacts` 成功后使对应 conversation key 和所有 folder key 失效。
- 错误不缓存；waiter 得到相同分类错误。
- cache 只存序列化结果，不持有数据库 transaction 或文件句柄。

后端保护是防御层，不能代替前端 render 循环修复。

### 5. 降低成功日志噪声

`list completed` 调整为：

- 正常成功记录 DEBUG。
- `elapsed_ms >= 100`、结果数异常大或 coordinator 失败时记录 INFO/WARN。
- 每分钟输出一次聚合指标：request count、coalesced count、cache hit、DB execution count、p95 elapsed。

不在热路径逐请求打印 INFO。

## 聚焦验证

前端：

1. 单个组件连续 render 100 次，同 key 只发起一次初始请求。
2. 两个组件同时订阅同 key，只发起一次请求并共享 snapshot。
3. 100 个 changed event 在 100 ms 内到达，只产生一次刷新。
4. changed 发生在 in-flight 时，完成后最多补一次刷新。
5. reconnect 只让每个活跃 key 刷新一次。
6. key 从 conversation 4 切到 9，旧请求结果不能覆盖新 key。

后端：

1. 50 个同 key 并发调用只执行一次 DB/文件状态扫描。
2. 250 ms cache 命中不重新执行扫描。
3. registration 后 cache 立即失效。
4. conversation 和 folder filter 不错误共享。

按仓库当前规则，实施时如未单独获批新增测试文件，至少用现有 lint/type check、静态调用链审查和开发版请求计数完成同等定向验证。

## 验收标准

- 无 Artifact 变化时，打开会话 10 分钟只发生初始查询，不持续轮询。
- 两个组件显示同一 conversation 时，后端 DB execution count 为 1。
- 事件突发下同 key 查询频率不超过 10 次/秒，正常目标为 1 次/突发。
- 运行日志不再出现连续每秒数百条 `list completed`。
- Artifact 新增、缺失状态变化、重连后的数据仍能在 1 秒内显示。

## 发布后验证

在新安装版复现原会话并持续运行两小时：

- 聚合指标中 request/coalesced/cache hit 合理，DB execution 不随 React render 增长。
- SQLite 写锁、文件状态扫描和日志体积明显下降。
- 同期重新采集内存压力；只有在查询风暴消失后，才使用结果校准方案 05。

## 风险与回滚

- cache 陈旧风险由 changed event、reconnect refresh 和 1 秒 stale time 控制。
- store 泄漏风险由 subscriber 计数和 60 秒淘汰处理。
- 可分两阶段回滚：先保留 primitive 依赖止血，关闭共享 cache；不得回滚到依赖 `filters` 对象的 effect。
