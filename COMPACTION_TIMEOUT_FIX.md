# 上下文压缩超时问题修复方案

## 问题描述

在处理长时间运行的复杂任务时，系统出现上下文压缩（Context Compaction）超时，导致连接突然断开。

### 根本原因

1. **硬编码的压缩预算**：原来的压缩预算固定为 300 秒（5分钟）
2. **重试机制耗尽**：网络不稳定 + API响应慢导致多次重试，最终超过预算
3. **级联失败**：压缩失败 → 流断开 → 连接终止 → 用户会话中断

### 日志证据

```
20:44:57 - compaction request budget exhausted; stopping retries
20:45:00 - Failed to run pre-sampling compact
20:45:02 - stream disconnected before completion (elapsed_ms: 304885)
20:45:02 - [ACP] prompt failed
20:45:02 - worker quit with fatal: transport terminated
```

## 已实施的修复

### 1. 增加压缩预算（立即生效）

**文件**: `./iyw-claw/harness/codex/patches/codex-core/src/compact_request_budget.rs`

**修改内容**:
```rust
// 从 300 秒增加到 600 秒
const COMPACTION_REQUEST_BUDGET: Duration = Duration::from_secs(600);
```

**影响**: 
- 压缩任务现在有 10 分钟的执行时间，而不是 5 分钟
- 大大降低了超时的可能性

### 2. 使预算可配置（支持动态调整）

**新增方法**:
```rust
impl RequestBudget {
    pub(crate) fn new() -> Self {
        Self::with_budget(COMPACTION_REQUEST_BUDGET)
    }

    pub(crate) fn with_budget(budget: Duration) -> Self {
        Self {
            deadline: Instant::now() + budget,
            budget,
        }
    }
}
```

**优势**:
- 未来可以根据任务复杂度动态调整预算
- 支持不同场景使用不同的超时设置

### 3. 增强日志和监控

**新增日志**:
```rust
// 成功时记录
tracing::info!(
    elapsed_ms = elapsed.as_millis(),
    retries = retries,
    "compaction completed successfully"
);

// 重试时记录
tracing::warn!(
    elapsed_ms = elapsed.as_millis(),
    retries = retries,
    max_retries = max_retries,
    error = %e,
    "compaction attempt failed, retrying after delay"
);

// 永久失败时记录
tracing::error!(
    elapsed_ms = elapsed.as_millis(),
    retries = retries,
    budget_exhausted = request_budget.is_exhausted(),
    error = %e,
    "compaction failed permanently"
);
```

**优势**:
- 更容易追踪压缩性能
- 可以识别网络问题 vs API性能问题
- 便于后续优化

## 部署步骤

### 1. 编译修改后的代码

```bash
cd ./iyw-claw
cargo build --release
```

### 2. 重启服务

```bash
# 停止当前服务
pkill -f iyw-claw

# 启动新版本
./target/release/iyw-claw
```

### 3. 验证修复

检查日志中的压缩时间：
```bash
tail -f /path/to/logs/iyw-claw.log | grep "compaction completed"
```

## 后续优化建议

### 短期优化（1-2周内）

1. **监控压缩性能**
   - 收集压缩耗时数据
   - 识别慢查询的模式
   - 分析重试原因

2. **网络层优化**
   - 检查 MCP 网关 `gateway.iyw.cn` 的稳定性
   - 考虑添加本地缓存
   - 优化 HTTP 连接复用

3. **智能重试策略**
   ```rust
   // 区分网络错误和 API 错误
   if is_network_error(&e) {
       // 网络错误：快速重试
       delay = Duration::from_secs(1);
   } else if is_api_slow(&e) {
       // API慢：增加预算而不是重试
       request_budget.extend(Duration::from_secs(60));
   }
   ```

### 中期优化（1-2个月内）

1. **渐进式压缩**
   - 不要等到超过限制才压缩
   - 当上下文达到 80% 时就开始压缩
   - 避免单次压缩任务过重

2. **压缩降级策略**
   ```rust
   // 如果远程压缩失败，尝试本地压缩
   match remote_compaction().await {
       Err(e) if e.is_timeout() => {
           tracing::warn!("remote compaction timeout, falling back to local");
           local_compaction().await?
       }
       result => result?
   }
   ```

3. **分段压缩**
   - 将大上下文分成多个小块
   - 并行压缩
   - 减少单次任务的压力

### 长期优化（3个月+）

1. **自适应预算**
   ```rust
   pub struct AdaptiveRequestBudget {
       base_budget: Duration,
       history_tokens: usize,
       complexity_factor: f64,
   }
   
   impl AdaptiveRequestBudget {
       pub fn calculate_budget(&self) -> Duration {
           let factor = 1.0 + (self.history_tokens as f64 / 100000.0) * self.complexity_factor;
           self.base_budget.mul_f64(factor)
       }
   }
   ```

2. **压缩算法优化**
   - 使用更快的模型进行压缩
   - 优化 prompt 设计
   - 缓存压缩结果

3. **配置化**
   - 在配置文件中暴露压缩预算
   - 允许用户根据需求调整
   - 支持环境变量覆盖

## 监控指标

关键指标需要追踪：

1. **压缩成功率**
   - 成功压缩数 / 总压缩尝试数
   - 目标：> 99%

2. **压缩耗时**
   - P50: < 30秒
   - P95: < 120秒
   - P99: < 300秒

3. **重试次数**
   - 平均重试次数
   - 目标：< 1 次

4. **预算使用率**
   - 实际耗时 / 预算时间
   - 目标：< 50%

## 回滚方案

如果修复导致问题：

```bash
cd ./iyw-claw
git checkout HEAD -- harness/codex/patches/codex-core/src/compact_request_budget.rs
git checkout HEAD -- harness/codex/patches/codex-core/src/compact.rs
cargo build --release
# 重启服务
```

## 联系方式

如有问题，请联系：
- 技术负责人：[填写]
- 紧急联系：[填写]
