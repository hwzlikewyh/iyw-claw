# 性能审计报告（performance.md）

> Audit A 只读基线 · 2026-08-01 · 结论先行：本轮未发现“已上线可量化回退”的性能缺陷，但性能目标整体未落地为可运行基准与观测指标（IYW-PERF-001）；桌面大列表渲染存在明确缺口（IYW-UI-001）；Fusion 中转已有连接池/快照/有界队列等正向设计。

## 1. 正向证据（已检查）
- **Fusion 中转**：上游 HTTP client 复用连接池并限制（`websearch/client.go:98-107`：MaxIdleConns 128 / MaxConnsPerHost 64 / IdleConnTimeout 90s / 15s 总超时）；运行快照经 `atomic.Pointer` 发布，请求热路径不访问 Apollo/MySQL/Redis/Nacos；记录用有界异步队列（LOG_QUEUE_SIZE=8192）。
- **并发下载**：桌面 `binary_cache.rs` 有全局/每主机上限、取消与 rename-aside trash 回收；`install.rs` 下载尝试有界（`PACKAGE_DOWNLOAD_ATTEMPTS`）。
- **前端列表**：`skill-market-data-list.tsx` 有请求序号防旧响应覆盖新请求（`requestId === context.request.current`）、250ms 防抖、分页。
- **relay 热路径**：JSON 用 sonic；body 只存 hash/size/512B preview。

## 2. 缺陷与缺口

### IYW-PERF-001（P2）—— 无基准与观测
- 现象：三仓无性能基准脚本、无 P50/P95/P99 观测埋点；设计文档要求的“启动、bootstrap、resolve、下载、安装、spawn、Skill 列表/详情 P50/P95/P99”“Fusion 不代理大文件后的出口字节验证”“64KiB/1MiB payload 与 artifact 阶梯测试”均无落地证据。
- 证据：`12-health-security-performance-audit.md` 性能章节（目标）；仓库内未发现 `bench`/`perf` 目录（Audit A 扫描）。
- owner：T12（工具脚本，本任务范围）→ 工具与基准脚本由 Audit B 交付；接线与指标导出由 T13。

### IYW-UI-001（P2）—— 大列表无虚拟滚动
- 现象：`skill-market-list.tsx`/`skill-market-data-list.tsx` 仅分页渲染，无 windowing；大 Skill 列表在弱设备上整页 DOM 渲染，滚动/首屏性能不达标。
- 证据：`src/components/skills/skill-market-data-list.tsx:56-77,187-234`（分页、无虚拟化）。
- owner：T10。

### 关注点（未定级，Audit B 动态验证）
- 桌面安装/更新下载超时策略：`update/version.rs:38-39` 注明“No overall request timeout”以保护慢速下载；需与“取消/重试/断点续传”一起做故障注入验证。
- `runtime_bootstrap.rs` 600s 下载超时与 15s 连接超时并存，需远端 CI 验证慢流/断网行为。
- 消息渠道轮询间隔（wecom poll interval）与 backpressure 需动态验证，本轮无运行时证据。

## 3. 待落地基准（Audit B 交付清单）
1. 脚本：`docs/audits/managed-distribution/scripts/bench_*`（非生产、不新增依赖、不改根配置）。
   - Fusion relay：64KiB / 1MiB 非流式 + 流式阶梯，对比基线（网关 P95 开销、转换分配）。
   - 桌面：启动/初始化/安装 P50/P95/P99 —— 只能远端 CI 运行。
2. 出口字节验证：Fusion 不代理大文件后，验证 TOS/CDN 承担 artifact 字节、Fusion 只发元数据与票据（IYW-SKILL-004/005 契约落地后）。
3. 泄漏检查：内存、句柄、临时文件、goroutine/task 泄漏（`go test -race` + pprof；桌面远端）。

## 4. 复现命令（Audit B 在允许环境运行）
```bash
# Fusion 定向基准（隔离环境）
go test ./internal/application/relay/... -bench=. -benchmem -run=^$ -race
# 中转阶梯（64KiB/1MiB 样本）需构造样本后对比 P95
# 桌面基准只能在远端 CI/测试机
```
