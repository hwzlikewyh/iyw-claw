# 上下文压缩长期优化路线图

## 🎯 核心问题

当前的压缩机制存在以下瓶颈：
1. **网络往返延迟**：每次压缩都需要调用远程 API（gpt-6-astra）
2. **串行处理**：压缩是同步阻塞操作
3. **重试代价高**：网络故障导致的重试非常耗时
4. **缺乏本地降级**：远程失败后没有备选方案

---

## 📊 方案对比：原生协议 vs 当前实现

### 当前实现（Remote Compaction V2）

```
用户请求 → 上下文累积 → 触发压缩
    ↓
调用远程 API (gpt-6-astra)
    ↓
等待响应（网络延迟 + API处理时间）
    ↓
失败重试（指数退避）
    ↓
超时 → 连接断开
```

**问题**：
- 网络依赖强
- 延迟不可控
- 成本高（每次压缩都是完整的 API 调用）

### 原生协议方案（推荐）

```
用户请求 → 上下文累积 → 触发压缩
    ↓
优先尝试：Claude/Codex 原生压缩协议
    ├─ 使用模型的内置 compaction 能力
    ├─ 直接在模型层面操作，无需额外 API 调用
    └─ 延迟低、成本低
    ↓
降级方案：本地启发式压缩
    ├─ 基于规则的消息合并
    ├─ Token 计数优化
    └─ 保留关键上下文
    ↓
兜底方案：当前的远程压缩
```

---

## 🏗️ 实施路线图

### 阶段 1：调研与设计（2-3周）

#### 1.1 调研 Claude/Codex 原生能力

**需要确认的问题**：

```markdown
1. Claude API 是否支持原生压缩？
   - 查看 Anthropic API 文档
   - 联系 Anthropic 技术支持
   - 测试 Messages API 的 system prompt caching

2. Codex 内部是否有压缩协议？
   - 检查现有代码中的 compact_remote_v2
   - 是否可以改进为原生实现

3. 模型层面的 context window 管理
   - 是否支持增量更新
   - 是否支持 context pruning
```

**调研输出**：
- 技术可行性报告
- API 对比分析
- 性能基准测试

#### 1.2 设计原生压缩协议

```rust
// 协议接口设计
pub trait CompressionProtocol {
    /// 原生压缩（最优）
    async fn native_compress(
        &self,
        context: &ConversationContext,
    ) -> Result<CompressedContext>;
    
    /// 本地启发式压缩（降级）
    async fn local_compress(
        &self,
        context: &ConversationContext,
    ) -> Result<CompressedContext>;
    
    /// 远程 API 压缩（兜底）
    async fn remote_compress(
        &self,
        context: &ConversationContext,
    ) -> Result<CompressedContext>;
}

// 自适应压缩策略
pub struct AdaptiveCompression {
    primary: Box<dyn CompressionProtocol>,
    fallback: Vec<Box<dyn CompressionProtocol>>,
    metrics: CompressionMetrics,
}

impl AdaptiveCompression {
    pub async fn compress(&self, context: &ConversationContext) -> Result<CompressedContext> {
        // 1. 尝试原生压缩
        match self.primary.native_compress(context).await {
            Ok(result) => {
                self.metrics.record_success("native");
                return Ok(result);
            }
            Err(e) if e.is_transient() => {
                // 临时错误，快速降级
            }
            Err(e) => {
                tracing::error!("native compression failed: {}", e);
            }
        }
        
        // 2. 尝试降级方案
        for fallback in &self.fallback {
            match fallback.local_compress(context).await {
                Ok(result) => {
                    self.metrics.record_success("fallback");
                    return Ok(result);
                }
                Err(e) => continue,
            }
        }
        
        // 3. 兜底：远程压缩
        self.primary.remote_compress(context).await
    }
}
```

---

### 阶段 2：实现原生压缩（4-6周）

#### 2.1 Claude Native Compression

如果 Anthropic 支持原生压缩，实现如下：

```rust
pub struct ClaudeNativeCompression {
    client: AnthropicClient,
    config: CompressionConfig,
}

impl CompressionProtocol for ClaudeNativeCompression {
    async fn native_compress(
        &self,
        context: &ConversationContext,
    ) -> Result<CompressedContext> {
        // 使用 Claude 的 prompt caching + context pruning
        let request = MessagesRequest {
            model: "claude-3-5-sonnet-20241022",
            messages: context.messages.clone(),
            system: vec![
                SystemBlock::Text {
                    text: "Summarize the conversation maintaining key context".to_string(),
                    cache_control: Some(CacheControl { type_: "ephemeral" }),
                }
            ],
            max_tokens: 4096,
            // 关键：使用缓存减少延迟
            ..Default::default()
        };
        
        let response = self.client.messages(request).await?;
        
        // 将压缩结果转换为标准格式
        Ok(CompressedContext {
            summary: response.content[0].text.clone(),
            preserved_messages: Self::extract_key_messages(context),
            tokens_saved: context.token_count() - response.usage.input_tokens,
        })
    }
}
```

#### 2.2 本地启发式压缩（不依赖 API）

```rust
pub struct LocalHeuristicCompression {
    config: HeuristicConfig,
}

impl CompressionProtocol for LocalHeuristicCompression {
    async fn local_compress(
        &self,
        context: &ConversationContext,
    ) -> Result<CompressedContext> {
        let mut compressed = CompressedContext::new();
        
        // 1. 保留最近的 N 条消息（始终保留）
        let recent_count = self.config.keep_recent_messages;
        let recent_msgs = context.messages.iter()
            .rev()
            .take(recent_count)
            .cloned()
            .collect();
        
        // 2. 识别并保留关键消息
        let key_messages = self.identify_key_messages(context);
        
        // 3. 对中间消息进行抽取式摘要
        let middle_summary = self.extractive_summarize(
            &context.messages[..context.messages.len() - recent_count]
        );
        
        // 4. 组合压缩结果
        compressed.summary = middle_summary;
        compressed.preserved_messages = key_messages;
        compressed.recent_messages = recent_msgs;
        
        Ok(compressed)
    }
    
    fn identify_key_messages(&self, context: &ConversationContext) -> Vec<Message> {
        context.messages.iter()
            .filter(|msg| {
                // 保留包含关键信息的消息
                self.contains_code_block(msg) ||
                self.contains_error(msg) ||
                self.is_user_directive(msg) ||
                self.has_tool_result(msg)
            })
            .cloned()
            .collect()
    }
    
    fn extractive_summarize(&self, messages: &[Message]) -> String {
        // 基于 TF-IDF 或其他启发式方法提取关键句子
        let key_sentences = messages.iter()
            .flat_map(|msg| self.extract_key_sentences(&msg.content))
            .collect::<Vec<_>>();
        
        key_sentences.join("\n")
    }
}
```

---

### 阶段 3：网络层优化（3-4周）

#### 3.1 连接池与复用

```rust
pub struct OptimizedHttpClient {
    pool: Pool<HttpConnection>,
    config: HttpConfig,
}

impl OptimizedHttpClient {
    pub async fn new(config: HttpConfig) -> Self {
        let pool = Pool::builder()
            .max_size(config.max_connections)
            .idle_timeout(Duration::from_secs(30))
            .connection_timeout(Duration::from_secs(5))
            .build(HttpConnectionManager::new(config.clone()))
            .await;
        
        Self { pool, config }
    }
    
    pub async fn request(&self, req: Request) -> Result<Response> {
        let mut conn = self.pool.get().await?;
        
        // HTTP/2 多路复用
        conn.send_request(req).await
    }
}
```

#### 3.2 智能重试策略

```rust
pub struct SmartRetry {
    strategy: RetryStrategy,
}

impl SmartRetry {
    pub async fn execute<F, T>(&self, mut operation: F) -> Result<T>
    where
        F: FnMut() -> BoxFuture<'static, Result<T>>,
    {
        let mut attempts = 0;
        let mut last_error = None;
        
        loop {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    attempts += 1;
                    last_error = Some(e.clone());
                    
                    // 区分错误类型
                    match self.classify_error(&e) {
                        ErrorClass::Transient => {
                            // 网络错误：快速重试
                            if attempts < 5 {
                                tokio::time::sleep(Duration::from_millis(100 * attempts)).await;
                                continue;
                            }
                        }
                        ErrorClass::RateLimited => {
                            // 限流：等待更长时间
                            if attempts < 3 {
                                tokio::time::sleep(Duration::from_secs(5)).await;
                                continue;
                            }
                        }
                        ErrorClass::ServerError => {
                            // 服务器错误：指数退避
                            if attempts < 4 {
                                let delay = Duration::from_secs(2u64.pow(attempts));
                                tokio::time::sleep(delay).await;
                                continue;
                            }
                        }
                        ErrorClass::Permanent => {
                            // 永久错误：立即失败
                            break;
                        }
                    }
                    
                    break;
                }
            }
        }
        
        Err(last_error.unwrap())
    }
}
```

#### 3.3 请求合并与批处理

```rust
pub struct RequestBatcher {
    pending: Arc<Mutex<Vec<CompactionRequest>>>,
    batch_size: usize,
    batch_timeout: Duration,
}

impl RequestBatcher {
    pub async fn submit(&self, request: CompactionRequest) -> Result<CompressedContext> {
        let (tx, rx) = oneshot::channel();
        
        {
            let mut pending = self.pending.lock().await;
            pending.push(RequestWithCallback { request, callback: tx });
            
            // 达到批次大小，立即处理
            if pending.len() >= self.batch_size {
                let batch = pending.drain(..).collect();
                tokio::spawn(self.process_batch(batch));
            }
        }
        
        rx.await?
    }
    
    async fn process_batch(&self, batch: Vec<RequestWithCallback>) {
        // 将多个压缩请求合并为一个 API 调用
        let combined_context = Self::merge_contexts(
            batch.iter().map(|r| &r.request.context)
        );
        
        let result = self.compress(combined_context).await;
        
        // 分发结果给各个请求
        for (request, callback) in batch {
            let individual_result = Self::extract_result(&result, &request);
            let _ = callback.send(individual_result);
        }
    }
}
```

---

### 阶段 4：渐进式压缩（2-3周）

不等到上下文满了才压缩，而是持续维护：

```rust
pub struct IncrementalCompressor {
    state: Arc<Mutex<CompressionState>>,
    config: IncrementalConfig,
}

impl IncrementalCompressor {
    pub async fn on_message_added(&self, message: Message) {
        let mut state = self.state.lock().await;
        
        state.add_message(message);
        
        // 检查是否需要压缩
        if self.should_compress(&state) {
            // 异步压缩，不阻塞当前请求
            let state_clone = state.clone();
            tokio::spawn(async move {
                if let Ok(compressed) = self.compress_incremental(&state_clone).await {
                    // 更新状态
                    state_clone.apply_compression(compressed);
                }
            });
        }
    }
    
    fn should_compress(&self, state: &CompressionState) -> bool {
        // 达到 80% 阈值就开始压缩
        let usage_ratio = state.token_count() as f64 / self.config.max_tokens as f64;
        usage_ratio > 0.8
    }
    
    async fn compress_incremental(&self, state: &CompressionState) -> Result<CompressedContext> {
        // 只压缩旧的部分，保留最近的消息
        let (old_messages, recent_messages) = state.split_at_recent(
            self.config.keep_recent_count
        );
        
        let compressed_old = self.compress_messages(old_messages).await?;
        
        Ok(CompressedContext {
            summary: compressed_old,
            recent_messages,
        })
    }
}
```

---

## 💰 成本收益分析

### 当前方案（Remote Compaction）

| 指标 | 值 |
|------|-----|
| 平均延迟 | 30-60秒 |
| 失败率 | ~5% |
| 成本/压缩 | $0.01-0.05 |
| 网络依赖 | 高 |

### 原生协议方案

| 指标 | 预估值 | 改善 |
|------|--------|------|
| 平均延迟 | 5-10秒 | **80-83% ↓** |
| 失败率 | <1% | **80% ↓** |
| 成本/压缩 | $0.001-0.01 | **90% ↓** |
| 网络依赖 | 低 | **显著改善** |

### 投资回报（ROI）

**开发投入**：约 3-4 人月
**年度节省**：
- 成本节省：约 50-70%
- 用户体验提升：断线率降低 80%
- 维护成本降低：故障点减少

**ROI 周期**：约 6-9 个月

---

## 🔄 迁移策略

### 分阶段灰度

```rust
pub struct CompactionRouter {
    native: NativeCompression,
    remote: RemoteCompression,
    rollout: RolloutConfig,
}

impl CompactionRouter {
    pub async fn compress(&self, context: &ConversationContext) -> Result<CompressedContext> {
        // 基于用户 ID 的百分比灰度
        let user_hash = hash(&context.user_id);
        let rollout_percentage = self.rollout.native_percentage;
        
        if user_hash % 100 < rollout_percentage {
            // 使用原生压缩
            match self.native.compress(context).await {
                Ok(result) => {
                    self.metrics.record("native_success");
                    return Ok(result);
                }
                Err(e) => {
                    self.metrics.record("native_failure");
                    tracing::warn!("native compression failed, falling back: {}", e);
                    // 降级到远程
                    return self.remote.compress(context).await;
                }
            }
        }
        
        // 其他用户仍使用远程压缩
        self.remote.compress(context).await
    }
}
```

### 灰度计划

| 阶段 | 流量占比 | 持续时间 | 观察指标 |
|------|----------|----------|----------|
| 内测 | 1% | 1周 | 基本功能 |
| 小规模 | 5% | 2周 | 稳定性、延迟 |
| 扩大 | 25% | 2周 | 成本、质量 |
| 半量 | 50% | 2周 | 全面指标 |
| 全量 | 100% | - | 持续监控 |

---

## 📈 监控与告警

### 关键指标

```rust
pub struct CompressionMetrics {
    // 性能指标
    pub latency_p50: Histogram,
    pub latency_p95: Histogram,
    pub latency_p99: Histogram,
    
    // 成功率
    pub success_rate: Counter,
    pub failure_rate: Counter,
    pub fallback_rate: Counter,
    
    // 质量指标
    pub compression_ratio: Histogram,
    pub context_preserved: Histogram,
    
    // 成本指标
    pub api_calls: Counter,
    pub tokens_consumed: Counter,
}

impl CompressionMetrics {
    pub fn record_compression(&self, result: &CompressionResult) {
        self.latency_p50.observe(result.duration.as_millis() as f64);
        
        if result.success {
            self.success_rate.inc();
        } else {
            self.failure_rate.inc();
        }
        
        if result.used_fallback {
            self.fallback_rate.inc();
        }
        
        let ratio = result.output_tokens as f64 / result.input_tokens as f64;
        self.compression_ratio.observe(ratio);
    }
}
```

### 告警规则

```yaml
alerts:
  - name: HighCompressionFailureRate
    expr: rate(compression_failure_total[5m]) > 0.05
    severity: critical
    message: "压缩失败率超过 5%"
  
  - name: HighCompressionLatency
    expr: histogram_quantile(0.95, compression_duration_seconds) > 60
    severity: warning
    message: "95% 压缩延迟超过 60 秒"
  
  - name: HighFallbackRate
    expr: rate(compression_fallback_total[5m]) > 0.2
    severity: warning
    message: "降级压缩使用率超过 20%"
```

---

## 🎓 推荐阅读

1. **Anthropic API 文档**
   - Prompt Caching: https://docs.anthropic.com/claude/docs/prompt-caching
   - Message Batching: https://docs.anthropic.com/claude/reference/messages_post

2. **论文与研究**
   - "Efficient Long-Context Language Models" 
   - "Compressing Transformers" (Google Research)

3. **最佳实践**
   - OpenAI Context Management
   - LangChain Memory Strategies

---

## ✅ 行动检查清单

### 立即行动（本周）
- [ ] 联系 Anthropic 技术支持，咨询原生压缩能力
- [ ] 分析现有 compact_remote_v2 代码
- [ ] 搭建性能基准测试环境

### 短期（1个月内）
- [ ] 完成技术可行性报告
- [ ] 设计原生压缩协议接口
- [ ] 实现本地启发式压缩原型
- [ ] 建立监控看板

### 中期（3个月内）
- [ ] 完成原生压缩实现
- [ ] 网络层优化上线
- [ ] 开始灰度测试（1% 流量）
- [ ] 收集反馈并迭代

### 长期（6个月内）
- [ ] 全量上线原生压缩
- [ ] 废弃旧的远程压缩（可选保留作为兜底）
- [ ] 优化成本和性能
- [ ] 文档和知识分享

---

## 💡 总结

**核心建议**：

1. **优先实现原生压缩协议**
   - 联系 Claude/Codex 团队确认 API 支持
   - 使用模型的内置能力，避免额外 API 调用

2. **建立多层降级机制**
   - 原生 → 本地启发式 → 远程 API
   - 确保任何一层失败都不会影响用户

3. **网络层优化是必要补充**
   - 连接复用、智能重试
   - 但不是核心解决方案

4. **渐进式部署**
   - 从 1% 灰度开始
   - 充分验证后再全量

**预期效果**：
- 延迟降低 80%+
- 成本降低 70%+
- 失败率降低 80%+
- 用户体验显著提升

需要我帮你深入展开某个具体部分吗？
