# Task 02：Fusion 持久化任务中心

## 目标

用 MySQL 5.7 兼容的租约任务中心替换分散的进程内 ticker，为 Skill ZIP、系统 Skill、Agent/CLI/运行时镜像和巡检提供可恢复执行基础。

## 已知现状

- `internal/bootstrap/skill_upload_cleanup.go` 和 `agent_release.go` 使用进程内 ticker。
- 进程重启会丢执行状态，多实例没有唯一 owner、租约和 fencing。
- 管理员无法查看 checkpoint、失败原因、重跑或死信。

## 依赖

- Task 01 已合并，`background_jobs/events` schema 和 contract revision 已冻结。
- 不等待 Task 03/04/05 的具体 handler；使用 job handler registry 解耦。

## scope_write

- 新增 `internal/domain/jobcenter/`。
- 新增 `internal/application/jobcenter/`。
- 新增 `internal/adapter/mysql/job_center_*.go`。
- 新增 `internal/adapter/httpserver/jobcenteradmin/` 的 handler 与 DTO，但不改 router。
- 新增聚焦测试和 fake clock/fake repository。
- 如需 bootstrap 装配，只新增独立 `internal/bootstrap/job_center.go` 导出构造函数；不得改总 bootstrap。

## 禁止修改

- SQL、router、根 bootstrap、Skill/agentrelease 业务代码、管理页、根依赖。

## 领域规则

- Job 类型使用编译内 registry；数据库 payload 不能指定任意 Go 类型、命令、URL或脚本。
- enqueue 以 `(job_type,dedupe_key)` 幂等；已成功任务是否允许新 generation 由业务显式决定。
- claim 使用单条条件 UPDATE：仅 pending/retry 且到期、或 running 且 lease 过期的任务可抢占。
- claim 成功递增 `fencing_token`；heartbeat/checkpoint/complete 必须携带 owner + token。
- 过期 worker 的任何写入返回 lease lost，不得覆盖新结果。
- retry 使用分类错误：transient、rate_limited、permanent、cancelled、integrity/security。
- integrity/security 直接 quarantine 或 dead，不自动无限重试。
- exponential backoff 有上限和 jitter；`max_attempts` 有合理默认和类型级覆盖。
- payload、checkpoint 和 error detail 有字节上限；写日志前脱敏。

## worker 生命周期

1. 固定数量 worker 轮询 due job，带随机抖动。
2. claim 后构造带 deadline/cancel 的 context。
3. handler 周期 heartbeat；长任务按稳定 checkpoint 恢复。
4. 成功先提交业务 side effect，再以 fencing token 标记 succeeded。
5. 失败分类并计算 next retry；超过尝试转 dead。
6. shutdown 停止 claim，取消可取消 handler，给活跃任务有限 drain；未完成租约自然过期。

## handler contract

```go
type Handler interface {
    Type() JobType
    ValidatePayload(json.RawMessage) error
    Run(ctx context.Context, lease Lease, progress ProgressWriter) Result
}
```

`ProgressWriter` 必须串行化 checkpoint，拒绝 token 不匹配。业务 handler 不能直接改 job 状态。

## 管理 API

- 列表筛选：type/status/time/owner/dedupe key，分页有上限。
- 详情：payload 只显示脱敏摘要，显示事件、checkpoint、attempt 和 error。
- retry：只允许 failed/dead，创建新 attempt 或显式重置，写审计。
- cancel：pending 直接取消，running 标记 cancellation requested，由 worker 协作退出。
- metrics：队列深度、最老等待、running、lease lost、重试、死信、成功率和耗时分位。

## 实施步骤

1. 先写状态转换和 backoff 领域测试。
2. 实现 repository 的 enqueue/claim/heartbeat/checkpoint/complete/fail。
3. 用 fake clock 验证租约过期、双 worker 抢占和 stale fencing。
4. 实现 worker pool、handler registry 和优雅退出。
5. 将现有两个 ticker 的业务抽成 handler 接口，但切换开关和总装配留给 Task 13。
6. 实现 admin handler 和 metrics snapshot。
7. 记录 job ID、type、attempt、fencing、耗时和结果；禁止记录完整 payload。

## 故障注入

- 进程在上传前、上传后数据库提交前、checkpoint 后退出。
- 两实例同时 claim 同一任务。
- heartbeat 延迟超过 lease，旧 worker 随后返回。
- MySQL 短暂断开、TOS 429/5xx、context cancel。
- payload 无效、摘要不匹配、任务被管理员取消。

## 验证

- 领域、application、MySQL repository 定向测试。
- 多 worker 竞争测试证明同一 fencing generation 只有一个有效提交者。
- 重启恢复测试证明 running 任务租约过期后可续跑。
- goroutine 数、队列和数据库轮询有界。
- `go test -race` 在可用环境对 jobcenter 包执行；不能执行时如实记录。

## 完成定义

- 任务可持久恢复、可重试、可取消、可审计、可观测。
- 新 job handler 无需修改 worker 主循环。
- 没有接线到总 router/bootstrap，也没有自行迁移业务 ticker。
