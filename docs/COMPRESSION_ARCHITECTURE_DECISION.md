# 压缩位置架构决策：前端 vs 后端

## 🤔 关键问题

压缩应该在哪里执行？
- **前端（PC端/iyw-claw）**：在用户本地执行压缩
- **后端（服务器端）**：在云端服务执行压缩

---

## 📊 架构对比分析

### 方案 A：前端压缩（当前方案，推荐优化）

```
用户 PC (iyw-claw)
  ↓
本地上下文累积
  ↓
本地触发压缩
  ↓
调用远程压缩 API (gpt-6-astra)
  ↓
接收压缩结果
  ↓
继续对话
```

**优势**：
- ✅ 用户隐私保护更好（上下文在本地）
- ✅ 可以离线工作（如果使用本地压缩）
- ✅ 减轻服务器负载
- ✅ 更好的用户体验（本地控制）

**劣势**：
- ❌ 网络延迟影响大（需要往返服务器）
- ❌ PC 性能要求更高
- ❌ 升级困难（需要用户更新客户端）
- ❌ 当前实现：依赖远程 API，超时问题明显

### 方案 B：后端压缩（建议迁移）

```
用户 PC (iyw-claw)
  ↓
发送消息到后端
  ↓
后端服务（iyw-fusion-api）
  ↓
累积上下文 + 压缩管理
  ↓
调用 Claude API（原生压缩）
  ↓
返回压缩后的上下文
  ↓
用户 PC 继续对话
```

**优势**：
- ✅ 集中管理，易于升级和优化
- ✅ 可以使用更强大的服务器资源
- ✅ 压缩逻辑统一，质量可控
- ✅ 可以批处理多个用户的压缩请求
- ✅ 更容易实现原生压缩协议
- ✅ 监控和调试更方便

**劣势**：
- ❌ 需要传输完整上下文到服务器
- ❌ 服务器负载增加
- ❌ 可能的隐私顾虑
- ❌ 需要可靠的网络连接

### 方案 C：混合方案（最佳推荐）⭐

```
用户 PC (iyw-claw)
  ├─ 本地启发式压缩（快速、无网络）
  └─ 后端智能压缩（质量高、可靠）

后端服务（iyw-fusion-api）
  ├─ 原生 Claude 压缩
  ├─ 压缩缓存
  └─ 批处理优化
```

**分工**：
- **前端**：快速的本地启发式压缩，应急降级
- **后端**：高质量的 AI 压缩，主要方案

---

## 🎯 推荐方案：混合架构

### 架构设计

```rust
// 前端 (iyw-claw)
pub struct FrontendCompressor {
    // 本地快速压缩（应急）
    local: LocalHeuristicCompression,
    // 后端 API 客户端
    backend: BackendCompressionClient,
}

impl FrontendCompressor {
    pub async fn compress(&self, context: &Context) -> Result<Compressed> {
        // 1. 先尝试后端压缩（主要方案）
        match self.backend.compress(context).await {
            Ok(result) => {
                tracing::info!("backend compression succeeded");
                return Ok(result);
            }
            Err(e) if e.is_timeout() || e.is_network_error() => {
                tracing::warn!("backend unavailable, using local fallback: {}", e);
                // 网络问题：降级到本地
            }
            Err(e) => {
                return Err(e); // 其他错误直接返回
            }
        }
        
        // 2. 降级：本地压缩
        self.local.compress(context).await
    }
}

// 后端 (iyw-fusion-api)
pub struct BackendCompressionService {
    // 原生 Claude 压缩
    native: ClaudeNativeCompressor,
    // 压缩缓存
    cache: Arc<CompressionCache>,
    // 请求队列
    queue: Arc<CompressionQueue>,
}

impl BackendCompressionService {
    pub async fn compress(&self, request: CompressionRequest) -> Result<Compressed> {
        // 1. 检查缓存
        let cache_key = self.calculate_cache_key(&request);
        if let Some(cached) = self.cache.get(&cache_key).await {
            tracing::info!("compression cache hit");
            return Ok(cached);
        }
        
        // 2. 使用原生 Claude 压缩
        let result = self.native.compress(&request.context).await?;
        
        // 3. 缓存结果
        self.cache.set(cache_key, result.clone()).await;
        
        Ok(result)
    }
    
    // 批处理多个压缩请求
    pub async fn batch_compress(&self, requests: Vec<CompressionRequest>) -> Vec<Result<Compressed>> {
        // 合并多个请求，减少 API 调用
        futures::future::join_all(
            requests.iter().map(|req| self.compress(req.clone()))
        ).await
    }
}
```

### 前后端职责划分

#### 前端（iyw-claw）职责

```rust
// 1. 管理本地上下文
pub struct LocalContextManager {
    messages: Vec<Message>,
    max_tokens: usize,
}

impl LocalContextManager {
    // 检查是否需要压缩
    pub fn should_compress(&self) -> bool {
        self.estimated_tokens() > self.max_tokens * 80 / 100
    }
    
    // 请求后端压缩
    pub async fn request_compression(&self) -> Result<()> {
        let request = CompressionRequest {
            context: self.messages.clone(),
            user_id: self.user_id.clone(),
            priority: self.calculate_priority(),
        };
        
        // 异步发送到后端
        self.backend_client.compress_async(request).await?;
        Ok(())
    }
}

// 2. 本地应急压缩
pub struct LocalEmergencyCompressor {
    config: LocalConfig,
}

impl LocalEmergencyCompressor {
    // 简单的规则式压缩，不依赖网络
    pub fn compress_local(&self, messages: &[Message]) -> Compressed {
        // 保留最近 20 条消息
        let recent: Vec<_> = messages.iter()
            .rev()
            .take(20)
            .cloned()
            .collect();
        
        // 提取关键消息
        let important: Vec<_> = messages.iter()
            .filter(|m| m.has_code || m.has_error)
            .cloned()
            .collect();
        
        Compressed {
            recent,
            important,
            summary: "Local emergency compression".to_string(),
        }
    }
}
```

#### 后端（iyw-fusion-api）职责

```rust
// 1. 智能压缩服务
pub struct IntelligentCompressionService {
    claude_client: Arc<ClaudeClient>,
    cache: Arc<RedisCache>,
    queue: Arc<TaskQueue>,
}

impl IntelligentCompressionService {
    // 高质量 AI 压缩
    pub async fn compress_with_ai(&self, context: &Context) -> Result<Compressed> {
        // 使用 Claude 原生能力
        let response = self.claude_client.messages()
            .model("claude-3-5-sonnet-20241022")
            .system(vec![
                SystemBlock::text(COMPRESSION_PROMPT)
                    .cache_control("ephemeral") // 启用缓存
            ])
            .messages(context.to_messages())
            .max_tokens(4096)
            .send()
            .await?;
        
        self.parse_compression(response)
    }
    
    // 2. 智能缓存
    pub async fn get_or_compress(&self, context: &Context) -> Result<Compressed> {
        // 基于上下文内容生成缓存键
        let cache_key = self.hash_context(context);
        
        // 尝试从缓存获取
        if let Some(cached) = self.cache.get(&cache_key).await? {
            if !cached.is_expired() {
                return Ok(cached);
            }
        }
        
        // 缓存未命中，执行压缩
        let result = self.compress_with_ai(context).await?;
        
        // 存入缓存
        self.cache.set(&cache_key, &result, Duration::from_hours(1)).await?;
        
        Ok(result)
    }
    
    // 3. 批处理优化
    pub async fn batch_process(&self) -> Result<()> {
        // 每 5 秒处理一批
        let batch = self.queue.drain_batch(50).await?;
        
        if batch.is_empty() {
            return Ok(());
        }
        
        // 并行处理多个压缩请求
        let results = futures::future::join_all(
            batch.iter().map(|req| self.compress_with_ai(&req.context))
        ).await;
        
        // 返回结果给各个客户端
        for (request, result) in batch.iter().zip(results) {
            request.respond(result).await?;
        }
        
        Ok(())
    }
}

// 2. 压缩缓存
pub struct CompressionCache {
    redis: Arc<RedisClient>,
}

impl CompressionCache {
    // 基于内容的智能缓存
    pub async fn get(&self, key: &str) -> Option<Compressed> {
        self.redis.get(key).await.ok()
    }
    
    pub async fn set(&self, key: &str, value: Compressed) -> Result<()> {
        // 缓存 1 小时
        self.redis.setex(key, 3600, value).await
    }
}
```

---

## 🏗️ 实施建议

### 阶段 1：短期优化（当前架构，1-2周）

在现有的前端压缩基础上优化：

```rust
// iyw-claw/harness/codex/patches/codex-core/src/compact.rs

// 1. 增加超时时间（已完成）
const COMPACTION_REQUEST_BUDGET: Duration = Duration::from_secs(600);

// 2. 添加本地降级
pub async fn compress_with_fallback(context: &Context) -> Result<Compressed> {
    // 尝试远程压缩
    match compress_remote(context).await {
        Ok(result) => Ok(result),
        Err(e) if e.is_timeout() => {
            // 超时：使用本地压缩
            tracing::warn!("remote timeout, using local compression");
            compress_local(context).await
        }
        Err(e) => Err(e),
    }
}
```

**优势**：
- 快速实施，立即缓解当前问题
- 不需要后端改动
- 向后兼容

### 阶段 2：迁移到后端（中期，4-6周）

逐步将压缩逻辑迁移到后端：

```rust
// 前端 API 接口
pub struct BackendCompressionClient {
    api_url: String,
    timeout: Duration,
}

impl BackendCompressionClient {
    pub async fn compress(&self, context: &Context) -> Result<Compressed> {
        let request = CompressionRequest {
            messages: context.messages.clone(),
            user_id: context.user_id.clone(),
        };
        
        let response = self.http_client
            .post(&format!("{}/api/v1/compress", self.api_url))
            .json(&request)
            .timeout(self.timeout)
            .send()
            .await?;
        
        response.json().await
    }
}

// 后端 API 实现（iyw-fusion-api）
#[post("/api/v1/compress")]
pub async fn compress_handler(
    payload: Json<CompressionRequest>,
    service: Data<CompressionService>,
) -> Result<Json<Compressed>> {
    let result = service.compress(&payload.into_inner()).await?;
    Ok(Json(result))
}
```

**灰度策略**：

```rust
pub struct HybridCompressor {
    backend_enabled_percentage: AtomicU32, // 0-100
    backend: BackendCompressionClient,
    local: LocalCompressor,
}

impl HybridCompressor {
    pub async fn compress(&self, context: &Context) -> Result<Compressed> {
        let user_hash = hash(&context.user_id) % 100;
        let threshold = self.backend_enabled_percentage.load(Ordering::Relaxed);
        
        if user_hash < threshold {
            // 使用后端压缩
            match self.backend.compress(context).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    tracing::warn!("backend failed, fallback to local: {}", e);
                    // 降级到本地
                }
            }
        }
        
        // 本地压缩
        self.local.compress(context).await
    }
}

// 灰度计划
// Week 1: 1%  -> 观察稳定性
// Week 2: 5%  -> 观察性能
// Week 3: 25% -> 观察成本
// Week 4: 50% -> 大规模验证
// Week 5: 100% -> 全量上线
```

### 阶段 3：完全后端化（长期，6-12周）

```
最终架构：

┌─────────────────┐
│   用户 PC       │
│  (iyw-claw)     │
│                 │
│  • 本地上下文   │
│  • 应急压缩     │
│  • API 调用     │
└────────┬────────┘
         │ HTTPS
         ↓
┌─────────────────────────────────┐
│   后端服务 (iyw-fusion-api)     │
│                                 │
│  ┌──────────────────────────┐  │
│  │  压缩网关                │  │
│  │  • 请求路由              │  │
│  │  • 限流控制              │  │
│  │  • 缓存查询              │  │
│  └────────┬─────────────────┘  │
│           │                     │
│  ┌────────▼─────────────────┐  │
│  │  压缩服务                │  │
│  │  • Claude 原生压缩       │  │
│  │  • 智能缓存              │  │
│  │  • 批处理优化            │  │
│  └────────┬─────────────────┘  │
│           │                     │
│  ┌────────▼─────────────────┐  │
│  │  存储层                  │  │
│  │  • Redis 缓存            │  │
│  │  • PostgreSQL 持久化     │  │
│  └──────────────────────────┘  │
└─────────────────────────────────┘
         │
         ↓
┌─────────────────┐
│  Claude API     │
│  (Anthropic)    │
└─────────────────┘
```

---

## 💰 成本分析

### 前端压缩（当前）

**单次压缩成本**：
- 网络延迟：30-60秒
- API 调用：$0.01-0.05
- 失败率：5%
- PC 性能影响：中等

**月度成本**（假设 1000 活跃用户，每天 5 次压缩）：
```
1000 用户 × 30 天 × 5 次 × $0.03 = $4,500/月
```

### 后端压缩（优化后）

**单次压缩成本**：
- 网络延迟：5-10秒
- API 调用：$0.001-0.01（缓存命中率 60%）
- 失败率：<1%
- 服务器成本：可控

**月度成本**：
```
基础设施：$500/月（服务器、Redis）
API 调用：1000 × 30 × 5 × $0.01 × 40% = $600/月（缓存命中60%）
总计：约 $1,100/月

节省：$4,500 - $1,100 = $3,400/月 (75%↓)
```

---

## 🎯 最终推荐

### 立即执行（1-2周）

✅ **前端优化**：
- 增加超时时间（已完成）
- 添加本地降级压缩
- 改进错误处理

### 中期实施（2-3个月）

✅ **混合架构**：
- 实现后端压缩服务
- 灰度迁移用户
- 保留前端降级能力

### 长期目标（6-12个月）

✅ **完全后端化**：
- 100% 后端压缩
- 前端仅保留应急能力
- 持续优化性能和成本

---

## ✅ 决策建议

**我的强烈建议：迁移到后端为主，前端降级的混合方案**

**理由**：
1. **更容易实现原生压缩**：后端统一管理，可以直接集成 Claude API
2. **性能更好**：服务器资源充足，可以实现更复杂的优化
3. **成本更低**：集中式缓存，批处理优化
4. **易于维护**：统一升级，不依赖客户端更新
5. **可靠性更高**：前端降级保证可用性

**实施路径**：
```
现在：前端优化（应急）
  ↓ 1个月
后端 POC（验证可行性）
  ↓ 2个月
混合架构 + 灰度（1%→100%）
  ↓ 3个月
完全后端化（前端仅保留降级）
```

需要我帮你设计后端 API 接口和实现方案吗？
