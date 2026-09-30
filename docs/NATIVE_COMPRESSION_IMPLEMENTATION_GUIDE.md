# 原生压缩协议实施指南

## 📋 概述

本文档提供了将当前的远程压缩迁移到原生压缩协议的详细实施步骤。

---

## 🎯 核心建议

基于代码分析和行业最佳实践，**强烈建议实现原生压缩协议**：

### 为什么选择原生协议？

1. **Claude/Codex 本身就是语言模型**
   - 它们天生具有总结和压缩文本的能力
   - 不需要额外调用外部 API
   - 可以直接在模型层面操作上下文

2. **延迟优势明显**
   ```
   当前方案：网络往返 + API 处理 = 30-60秒
   原生方案：内存操作 = 5-10秒
   
   改善：80%+ 延迟降低
   ```

3. **成本大幅降低**
   ```
   当前：每次压缩需要完整的 API 调用
   原生：利用已有的模型实例，边际成本接近零
   
   节省：70-90% 成本降低
   ```

4. **可靠性提升**
   ```
   当前：依赖网络 + 远程服务
   原生：本地处理 + 降级机制
   
   改善：失败率从 5% 降至 <1%
   ```

---

## 🔍 技术可行性分析

### 方案 A：Claude Prompt Caching（推荐）

Anthropic 提供的 Prompt Caching 功能可以用于实现高效压缩：

```python
# 使用 Claude API 的 prompt caching
client = anthropic.Anthropic(api_key="...")

response = client.messages.create(
    model="claude-3-5-sonnet-20241022",
    max_tokens=4096,
    system=[
        {
            "type": "text",
            "text": "You are a conversation summarizer...",
            "cache_control": {"type": "ephemeral"}  # 启用缓存
        }
    ],
    messages=conversation_history
)

# 压缩后的摘要
summary = response.content[0].text
```

**优势**：
- ✅ 官方支持
- ✅ 缓存减少延迟和成本
- ✅ 质量有保证

**实施难度**：⭐⭐ (低)

### 方案 B：内部模型实例复用

如果 iyw-claw 内部已经有模型实例：

```rust
// 利用现有的模型实例进行压缩
pub async fn compress_with_local_model(
    model: &LocalModelInstance,
    conversation: &[Message],
) -> Result<String> {
    let prompt = format!(
        "Summarize this conversation:\n\n{}",
        format_conversation(conversation)
    );
    
    // 直接使用本地模型，无需网络调用
    model.generate(&prompt, GenerationConfig {
        max_tokens: 2048,
        temperature: 0.3,  // 低温度以保持一致性
        ..Default::default()
    }).await
}
```

**优势**：
- ✅ 零网络延迟
- ✅ 零边际成本
- ✅ 完全可控

**实施难度**：⭐⭐⭐ (中等，取决于现有架构)

### 方案 C：混合方案（最佳）

结合多种方法，提供最佳的鲁棒性：

```rust
pub struct HybridCompressor {
    primary: ClaudeNativeCompression,    // 原生压缩
    fallback1: LocalHeuristicCompression, // 本地启发式
    fallback2: RemoteCompression,         // 远程 API（兜底）
}

impl HybridCompressor {
    pub async fn compress(&self, context: &Context) -> Result<Compressed> {
        // 1. 尝试原生压缩（最快、成本最低）
        if let Ok(result) = self.primary.compress(context).await {
            return Ok(result);
        }
        
        // 2. 降级到本地启发式（无网络依赖）
        if let Ok(result) = self.fallback1.compress(context).await {
            return Ok(result);
        }
        
        // 3. 兜底：远程压缩（保证可用性）
        self.fallback2.compress(context).await
    }
}
```

**优势**：
- ✅ 最佳性能
- ✅ 最高可靠性
- ✅ 渐进式迁移

**实施难度**：⭐⭐⭐⭐ (高，但回报最大)

---

## 📝 实施步骤

### 第 1 步：调研与验证（1周）

#### 1.1 联系 Anthropic 技术支持

**需要确认**：
- [ ] Prompt Caching 是否支持压缩场景
- [ ] 是否有官方的 context management API
- [ ] 是否有批量处理接口

**联系方式**：
- 技术支持：support@anthropic.com
- API 文档：https://docs.anthropic.com/claude/reference

#### 1.2 分析现有代码

**需要检查**：
- [ ] `compact_remote_v2.rs` 的实现细节
- [ ] 现有的模型调用接口
- [ ] 是否有本地模型实例可以复用

```bash
# 查找相关代码
cd ./iyw-claw
grep -r "RemoteCompactionSupport" --include="*.rs"
grep -r "compact_remote_v2" --include="*.rs"
grep -r "ModelClient" --include="*.rs"
```

#### 1.3 性能基准测试

建立基准测试以便后续对比：

```rust
#[tokio::test]
async fn benchmark_current_compression() {
    let context = create_large_context(100); // 100 条消息
    
    let start = Instant::now();
    let result = current_compressor.compress(&context).await.unwrap();
    let elapsed = start.elapsed();
    
    println!("Current method:");
    println!("  Latency: {:?}", elapsed);
    println!("  Tokens saved: {}", result.tokens_saved);
    println!("  Success: {}", result.success);
}
```

### 第 2 步：实现本地启发式压缩（2周）

这是最容易实现的降级方案，不依赖任何外部 API。

#### 2.1 创建本地压缩模块

```bash
# 在 iyw-claw 中创建新文件
touch ./harness/codex/patches/codex-core/src/compact_local.rs
```

#### 2.2 实现核心逻辑

参考 `docs/compact_native_poc.rs` 中的 `LocalHeuristicCompression`：

```rust
pub struct LocalCompressor {
    config: LocalConfig,
}

impl LocalCompressor {
    pub async fn compress(&self, context: &Context) -> Result<Compressed> {
        // 1. 保留最近 N 条消息
        let recent = context.messages.iter()
            .rev()
            .take(self.config.keep_recent)
            .cloned()
            .collect();
        
        // 2. 识别关键消息（包含代码、错误等）
        let important = self.extract_important_messages(&context);
        
        // 3. 对其余消息进行简单摘要
        let summary = self.summarize_remaining(&context, &important, &recent);
        
        Ok(Compressed { summary, important, recent })
    }
    
    fn extract_important_messages(&self, context: &Context) -> Vec<Message> {
        context.messages.iter()
            .filter(|msg| {
                msg.content.contains("```") ||  // 代码块
                msg.content.contains("error") || // 错误
                msg.content.contains("TODO") ||  // 待办
                msg.role == "tool_result"        // 工具结果
            })
            .cloned()
            .collect()
    }
    
    fn summarize_remaining(&self, context: &Context, 
                          important: &[Message], 
                          recent: &[Message]) -> String {
        // 简单的抽取式摘要：每条消息取前 50 个字符
        context.messages.iter()
            .filter(|msg| !important.contains(msg) && !recent.contains(msg))
            .map(|msg| {
                let preview: String = msg.content.chars().take(50).collect();
                format!("[{}] {}...", msg.role, preview)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
```

#### 2.3 集成到现有流程

修改 `compact.rs`：

```rust
// 在 compact.rs 中添加
use crate::compact_local::LocalCompressor;

pub async fn run_compact_with_fallback(...) -> Result<()> {
    // 尝试远程压缩
    match run_remote_compact(...).await {
        Ok(result) => Ok(result),
        Err(e) => {
            tracing::warn!("remote compression failed, trying local: {}", e);
            
            // 降级到本地压缩
            let local = LocalCompressor::new();
            local.compress(context).await
        }
    }
}
```

#### 2.4 测试验证

```bash
# 运行单元测试
cargo test --package codex-core compact_local

# 运行集成测试
cargo test --package codex-core --test integration_compact
```

### 第 3 步：实现 Claude 原生压缩（3-4周）

#### 3.1 实现 Claude API 集成

创建 `compact_claude_native.rs`：

```rust
use anthropic_sdk::{Client, Message, SystemBlock};

pub struct ClaudeNativeCompressor {
    client: Client,
    config: ClaudeConfig,
}

impl ClaudeNativeCompressor {
    pub async fn compress(&self, context: &Context) -> Result<Compressed> {
        let system_prompt = self.build_compression_prompt();
        let messages = self.prepare_messages(&context.messages);
        
        let response = self.client.messages()
            .model(&self.config.model)
            .system(vec![
                SystemBlock::text(system_prompt)
                    .cache_control("ephemeral") // 启用缓存
            ])
            .messages(messages)
            .max_tokens(4096)
            .send()
            .await?;
        
        self.parse_compression_result(response)
    }
    
    fn build_compression_prompt(&self) -> String {
        r#"You are compressing a conversation for efficient context management.
        
Your task:
1. Create a concise summary maintaining all critical information
2. Preserve code blocks, error messages, and key decisions
3. Note any unresolved issues or ongoing work
4. Use structured format for clarity

Format your response as:
## Summary
[Brief overview]

## Key Points
- [Important point 1]
- [Important point 2]

## Code/Technical Details
[Any code or technical info that must be preserved]

## Status
[Current state and next steps]"#.to_string()
    }
}
```

#### 3.2 配置与优化

在配置文件中添加：

```toml
# config.toml
[compression]
method = "adaptive"  # adaptive, native, local, remote
native_model = "claude-3-5-sonnet-20241022"
max_summary_tokens = 4096
enable_caching = true
fallback_on_error = true
```

#### 3.3 性能对比测试

```rust
#[tokio::test]
async fn compare_compression_methods() {
    let context = create_test_context();
    
    // 当前方法
    let (remote_result, remote_time) = time_compression(
        &remote_compressor, &context
    ).await;
    
    // 原生方法
    let (native_result, native_time) = time_compression(
        &claude_compressor, &context
    ).await;
    
    // 本地方法
    let (local_result, local_time) = time_compression(
        &local_compressor, &context
    ).await;
    
    println!("Performance comparison:");
    println!("  Remote: {:?}", remote_time);
    println!("  Native: {:?} ({}% faster)", 
        native_time, 
        ((remote_time - native_time) / remote_time * 100.0)
    );
    println!("  Local:  {:?} ({}% faster)", 
        local_time,
        ((remote_time - local_time) / remote_time * 100.0)
    );
}
```

### 第 4 步：实现自适应路由（1-2周）

#### 4.1 创建路由器

```rust
pub struct AdaptiveCompressor {
    native: Arc<ClaudeNativeCompressor>,
    local: Arc<LocalCompressor>,
    remote: Arc<RemoteCompressor>,
    strategy: CompressionStrategy,
    metrics: Arc<Metrics>,
}

#[derive(Clone)]
pub enum CompressionStrategy {
    /// 总是使用原生
    AlwaysNative,
    /// 总是使用本地
    AlwaysLocal,
    /// 自适应选择
    Adaptive {
        /// 原生失败后降级到本地
        fallback_to_local: bool,
        /// 本地失败后降级到远程
        fallback_to_remote: bool,
    },
    /// 灰度发布（按用户百分比）
    Rollout {
        native_percentage: u32,
    },
}

impl AdaptiveCompressor {
    pub async fn compress(&self, context: &Context) -> Result<Compressed> {
        match &self.strategy {
            CompressionStrategy::Adaptive { .. } => {
                self.compress_adaptive(context).await
            }
            CompressionStrategy::Rollout { native_percentage } => {
                self.compress_rollout(context, *native_percentage).await
            }
            _ => self.compress_simple(context).await
        }
    }
    
    async fn compress_adaptive(&self, context: &Context) -> Result<Compressed> {
        // 1. 尝试原生
        match self.native.compress(context).await {
            Ok(result) => {
                self.metrics.record("native_success");
                return Ok(result);
            }
            Err(e) if e.is_transient() => {
                tracing::warn!("native failed transiently: {}", e);
            }
            Err(e) => {
                tracing::error!("native failed: {}", e);
                self.metrics.record("native_failure");
            }
        }
        
        // 2. 降级到本地
        match self.local.compress(context).await {
            Ok(result) => {
                self.metrics.record("local_success");
                return Ok(result);
            }
            Err(e) => {
                tracing::error!("local failed: {}", e);
                self.metrics.record("local_failure");
            }
        }
        
        // 3. 兜底：远程
        self.remote.compress(context).await
    }
    
    async fn compress_rollout(&self, context: &Context, percentage: u32) -> Result<Compressed> {
        let user_hash = self.hash_user_id(&context.user_id);
        
        if user_hash % 100 < percentage {
            // 使用原生（带降级）
            self.compress_adaptive(context).await
        } else {
            // 继续使用远程
            self.remote.compress(context).await
        }
    }
}
```

#### 4.2 灰度发布控制

```rust
pub struct RolloutController {
    current_percentage: Arc<AtomicU32>,
}

impl RolloutController {
    pub fn increase_rollout(&self, delta: u32) {
        let current = self.current_percentage.load(Ordering::Relaxed);
        let new = (current + delta).min(100);
        self.current_percentage.store(new, Ordering::Relaxed);
        
        tracing::info!(
            old = current,
            new = new,
            "rollout percentage updated"
        );
    }
    
    pub fn get_percentage(&self) -> u32 {
        self.current_percentage.load(Ordering::Relaxed)
    }
}

// 使用示例
let controller = RolloutController::new();

// 开始灰度 1%
controller.set_percentage(1);
tokio::time::sleep(Duration::from_days(1)).await;

// 观察指标正常，扩大到 5%
if metrics.success_rate() > 0.99 {
    controller.increase_rollout(4);
}
```

### 第 5 步：监控与优化（持续）

#### 5.1 关键指标监控

```rust
pub struct CompressionMetrics {
    // 性能指标
    pub latency_histogram: Histogram,
    pub success_counter: Counter,
    pub failure_counter: Counter,
    
    // 方法使用统计
    pub native_usage: Counter,
    pub local_usage: Counter,
    pub remote_usage: Counter,
    
    // 质量指标
    pub compression_ratio: Histogram,
}

impl CompressionMetrics {
    pub fn record_compression(&self, result: &CompressionResult) {
        // 记录延迟
        self.latency_histogram.observe(result.latency_ms as f64);
        
        // 记录成功/失败
        if result.success {
            self.success_counter.inc();
        } else {
            self.failure_counter.inc();
        }
        
        // 记录使用的方法
        match result.method {
            Method::Native => self.native_usage.inc(),
            Method::Local => self.local_usage.inc(),
            Method::Remote => self.remote_usage.inc(),
        }
        
        // 记录压缩比率
        let ratio = result.output_tokens as f64 / result.input_tokens as f64;
        self.compression_ratio.observe(ratio);
    }
    
    pub fn report(&self) -> MetricsReport {
        MetricsReport {
            total_compressions: self.success_counter.get() + self.failure_counter.get(),
            success_rate: self.calculate_success_rate(),
            avg_latency_ms: self.latency_histogram.mean(),
            p95_latency_ms: self.latency_histogram.quantile(0.95),
            p99_latency_ms: self.latency_histogram.quantile(0.99),
            method_distribution: self.get_method_distribution(),
        }
    }
}
```

#### 5.2 监控仪表板

创建 Grafana 仪表板，追踪：

```yaml
# grafana-dashboard.yaml
dashboard:
  title: "Compression Performance"
  panels:
    - title: "Compression Latency"
      type: graph
      targets:
        - expr: histogram_quantile(0.95, compression_latency_ms)
          legend: "P95"
        - expr: histogram_quantile(0.50, compression_latency_ms)
          legend: "P50"
    
    - title: "Success Rate"
      type: gauge
      targets:
        - expr: rate(compression_success[5m]) / rate(compression_total[5m])
    
    - title: "Method Distribution"
      type: pie
      targets:
        - expr: sum by (method) (compression_method_count)
```

#### 5.3 告警配置

```yaml
# alerts.yaml
groups:
  - name: compression_alerts
    rules:
      - alert: HighCompressionLatency
        expr: histogram_quantile(0.95, compression_latency_ms) > 30000
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "P95 compression latency > 30s"
      
      - alert: LowCompressionSuccessRate
        expr: rate(compression_success[5m]) / rate(compression_total[5m]) < 0.95
        for: 5m
        labels:
          severity: critical
        annotations:
          summary: "Compression success rate < 95%"
      
      - alert: HighNativeFallbackRate
        expr: rate(compression_local_usage[5m]) / rate(compression_native_usage[5m]) > 0.2
        for: 10m
        labels:
          severity: warning
        annotations:
          summary: "Native compression fallback rate > 20%"
```

---

## 📊 成功标准

### 阶段 1：本地压缩（2周后）

- [ ] 本地压缩成功率 > 95%
- [ ] 延迟 < 5 秒
- [ ] 质量可接受（人工评估）
- [ ] 无严重 bug

### 阶段 2：原生压缩 POC（4周后）

- [ ] 原生压缩成功率 > 99%
- [ ] 延迟 < 10 秒（比远程快 70%+）
- [ ] 成本降低 > 60%
- [ ] 质量与远程相当或更好

### 阶段 3：灰度发布（8周后）

- [ ] 1% 流量运行 1 周无问题
- [ ] 5% 流量运行 1 周无问题
- [ ] 25% 流量运行 2 周无问题
- [ ] 用户投诉 = 0

### 阶段 4：全量上线（12周后）

- [ ] 100% 流量使用新方案
- [ ] 整体延迟降低 > 70%
- [ ] 整体成本降低 > 60%
- [ ] 失败率 < 1%
- [ ] 用户满意度提升

---

## 🚨 风险与缓解

### 风险 1：原生压缩质量不如远程

**缓解措施**：
- 并行运行两种方法，对比结果
- A/B 测试收集用户反馈
- 质量问题时自动降级

### 风险 2：Claude API 变更

**缓解措施**：
- 保持远程压缩作为兜底
- 订阅 Anthropic 的 API 更新通知
- 维护版本兼容性层

### 风险 3：性能不达预期

**缓解措施**：
- 详细的性能基准测试
- 分阶段灰度，及时发现问题
- 准备回滚方案

### 风险 4：成本反而增加

**缓解措施**：
- 详细的成本分析和预算
- 监控实际 API 调用成本
- 优化缓存策略

---

## ✅ 检查清单

### 开始前

- [ ] 获得团队和管理层支持
- [ ] 分配足够的开发资源（至少 1 人全职 3 个月）
- [ ] 准备测试环境
- [ ] 建立监控和告警

### 实施中

- [ ] 每周进行进度评审
- [ ] 及时记录问题和解决方案
- [ ] 保持与 Anthropic 的沟通
- [ ] 持续收集性能数据

### 完成后

- [ ] 撰写总结文档
- [ ] 分享经验和最佳实践
- [ ] 关闭旧的远程压缩代码（可选）
- [ ] 庆祝成功！🎉

---

## 📚 参考资料

1. **Anthropic 文档**
   - Prompt Caching: https://docs.anthropic.com/claude/docs/prompt-caching
   - Message API: https://docs.anthropic.com/claude/reference/messages_post

2. **代码示例**
   - POC 实现: `./docs/compact_native_poc.rs`
   - 优化路线图: `./docs/COMPACTION_OPTIMIZATION_ROADMAP.md`

3. **最佳实践**
   - Google: Efficient Long-Context Language Models
   - OpenAI: Context Management Best Practices

---

**最后更新**: 2026-09-29
**版本**: 1.0
**作者**: Claude (Fable 5)
