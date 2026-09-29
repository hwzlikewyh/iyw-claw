// 原生压缩协议 POC 实现
// 文件路径: ./iyw-claw/harness/codex/patches/codex-core/src/compact_native.rs

use std::sync::Arc;
use std::time::{Duration, Instant};
use async_trait::async_trait;
use codex_protocol::error::{CodexErr, Result as CodexResult};
use serde::{Deserialize, Serialize};

/// 压缩后的上下文
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressedContext {
    /// 压缩摘要
    pub summary: String,
    /// 保留的关键消息
    pub preserved_messages: Vec<Message>,
    /// 最近的消息（始终保留）
    pub recent_messages: Vec<Message>,
    /// 节省的 token 数
    pub tokens_saved: usize,
    /// 压缩方法
    pub method: CompressionMethod,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompressionMethod {
    Native,      // 原生压缩
    Local,       // 本地启发式
    Remote,      // 远程 API
}

#[derive(Debug, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
    pub metadata: MessageMetadata,
}

#[derive(Debug, Clone)]
pub struct MessageMetadata {
    pub has_code: bool,
    pub has_error: bool,
    pub is_directive: bool,
    pub token_count: usize,
}

/// 压缩协议接口
#[async_trait]
pub trait CompressionProtocol: Send + Sync {
    /// 原生压缩（最优先）
    async fn native_compress(
        &self,
        context: &ConversationContext,
    ) -> CodexResult<CompressedContext>;

    /// 本地压缩（降级方案）
    async fn local_compress(
        &self,
        context: &ConversationContext,
    ) -> CodexResult<CompressedContext>;

    /// 远程压缩（兜底方案）
    async fn remote_compress(
        &self,
        context: &ConversationContext,
    ) -> CodexResult<CompressedContext>;

    /// 获取协议名称
    fn name(&self) -> &str;
}

/// 会话上下文
#[derive(Debug, Clone)]
pub struct ConversationContext {
    pub messages: Vec<Message>,
    pub total_tokens: usize,
    pub user_id: String,
}

impl ConversationContext {
    pub fn token_count(&self) -> usize {
        self.total_tokens
    }
}

/// Claude 原生压缩实现
pub struct ClaudeNativeCompression {
    client: Arc<AnthropicClient>,
    config: NativeCompressionConfig,
}

#[derive(Debug, Clone)]
pub struct NativeCompressionConfig {
    pub model: String,
    pub max_summary_tokens: usize,
    pub keep_recent_messages: usize,
}

impl Default for NativeCompressionConfig {
    fn default() -> Self {
        Self {
            model: "claude-3-5-sonnet-20241022".to_string(),
            max_summary_tokens: 4096,
            keep_recent_messages: 10,
        }
    }
}

#[async_trait]
impl CompressionProtocol for ClaudeNativeCompression {
    async fn native_compress(
        &self,
        context: &ConversationContext,
    ) -> CodexResult<CompressedContext> {
        let start = Instant::now();

        // 分离最近的消息
        let (to_compress, recent) = self.split_messages(&context.messages);

        if to_compress.is_empty() {
            return Ok(CompressedContext {
                summary: String::new(),
                preserved_messages: Vec::new(),
                recent_messages: recent,
                tokens_saved: 0,
                method: CompressionMethod::Native,
            });
        }

        // 使用 Claude API 的 prompt caching 功能
        let summary = self.compress_with_claude(&to_compress).await?;

        // 识别需要保留的关键消息
        let preserved = self.identify_key_messages(&to_compress);

        let elapsed = start.elapsed();
        tracing::info!(
            elapsed_ms = elapsed.as_millis(),
            messages_compressed = to_compress.len(),
            messages_preserved = preserved.len(),
            "native compression completed"
        );

        Ok(CompressedContext {
            summary,
            preserved_messages: preserved,
            recent_messages: recent,
            tokens_saved: self.calculate_tokens_saved(&to_compress, &summary),
            method: CompressionMethod::Native,
        })
    }

    async fn local_compress(
        &self,
        context: &ConversationContext,
    ) -> CodexResult<CompressedContext> {
        // 降级到本地压缩
        let local = LocalHeuristicCompression::new();
        local.compress(context).await
    }

    async fn remote_compress(
        &self,
        context: &ConversationContext,
    ) -> CodexResult<CompressedContext> {
        // 兜底：使用现有的远程压缩
        Err(CodexErr::Stream(
            "Remote compression not implemented in POC".to_string()
        ))
    }

    fn name(&self) -> &str {
        "claude_native"
    }
}

impl ClaudeNativeCompression {
    fn split_messages(&self, messages: &[Message]) -> (Vec<Message>, Vec<Message>) {
        let keep_count = self.config.keep_recent_messages;
        if messages.len() <= keep_count {
            return (Vec::new(), messages.to_vec());
        }

        let split_point = messages.len() - keep_count;
        (
            messages[..split_point].to_vec(),
            messages[split_point..].to_vec(),
        )
    }

    async fn compress_with_claude(&self, messages: &[Message]) -> CodexResult<String> {
        // 构建压缩 prompt
        let system_prompt = r#"You are a conversation summarizer.
Create a concise but comprehensive summary of the conversation that:
1. Preserves all key technical details (code, errors, decisions)
2. Maintains the logical flow of the discussion
3. Highlights unresolved issues or ongoing work
4. Uses bullet points for clarity

Focus on information that would be needed to continue the conversation meaningfully."#;

        let conversation_text = messages.iter()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n\n");

        // 调用 Claude API
        let response = self.client.compress(
            &self.config.model,
            system_prompt,
            &conversation_text,
            self.config.max_summary_tokens,
        ).await?;

        Ok(response)
    }

    fn identify_key_messages(&self, messages: &[Message]) -> Vec<Message> {
        messages.iter()
            .filter(|msg| {
                msg.metadata.has_code ||
                msg.metadata.has_error ||
                msg.metadata.is_directive
            })
            .cloned()
            .collect()
    }

    fn calculate_tokens_saved(&self, original: &[Message], summary: &str) -> usize {
        let original_tokens: usize = original.iter()
            .map(|m| m.metadata.token_count)
            .sum();

        let summary_tokens = summary.len() / 4; // 粗略估计

        original_tokens.saturating_sub(summary_tokens)
    }
}

/// 本地启发式压缩（无 API 调用）
pub struct LocalHeuristicCompression {
    config: HeuristicConfig,
}

#[derive(Debug, Clone)]
pub struct HeuristicConfig {
    pub keep_recent_messages: usize,
    pub min_importance_score: f64,
}

impl Default for HeuristicConfig {
    fn default() -> Self {
        Self {
            keep_recent_messages: 10,
            min_importance_score: 0.5,
        }
    }
}

impl LocalHeuristicCompression {
    pub fn new() -> Self {
        Self {
            config: HeuristicConfig::default(),
        }
    }

    pub async fn compress(&self, context: &ConversationContext) -> CodexResult<CompressedContext> {
        let start = Instant::now();

        // 1. 分离最近消息
        let (old_messages, recent_messages) = self.split_messages(&context.messages);

        // 2. 识别关键消息
        let key_messages = self.identify_key_messages(&old_messages);

        // 3. 对剩余消息进行抽取式摘要
        let summary = self.extractive_summarize(&old_messages);

        let elapsed = start.elapsed();
        tracing::info!(
            elapsed_ms = elapsed.as_millis(),
            messages_compressed = old_messages.len(),
            key_messages = key_messages.len(),
            "local heuristic compression completed"
        );

        Ok(CompressedContext {
            summary,
            preserved_messages: key_messages,
            recent_messages,
            tokens_saved: self.estimate_tokens_saved(&old_messages),
            method: CompressionMethod::Local,
        })
    }

    fn split_messages(&self, messages: &[Message]) -> (Vec<Message>, Vec<Message>) {
        let keep_count = self.config.keep_recent_messages;
        if messages.len() <= keep_count {
            return (Vec::new(), messages.to_vec());
        }

        let split_point = messages.len() - keep_count;
        (
            messages[..split_point].to_vec(),
            messages[split_point..].to_vec(),
        )
    }

    fn identify_key_messages(&self, messages: &[Message]) -> Vec<Message> {
        messages.iter()
            .filter(|msg| {
                let score = self.calculate_importance(msg);
                score >= self.config.min_importance_score
            })
            .cloned()
            .collect()
    }

    fn calculate_importance(&self, msg: &Message) -> f64 {
        let mut score = 0.0;

        // 包含代码块 +0.3
        if msg.metadata.has_code {
            score += 0.3;
        }

        // 包含错误 +0.4
        if msg.metadata.has_error {
            score += 0.4;
        }

        // 是用户指令 +0.5
        if msg.metadata.is_directive {
            score += 0.5;
        }

        // 消息长度 +0.1 (长消息通常更重要)
        if msg.metadata.token_count > 200 {
            score += 0.1;
        }

        score.min(1.0)
    }

    fn extractive_summarize(&self, messages: &[Message]) -> String {
        // 简单的抽取式摘要：提取每条消息的前 100 个字符
        messages.iter()
            .filter_map(|msg| {
                if self.calculate_importance(msg) < self.config.min_importance_score {
                    let preview = msg.content.chars().take(100).collect::<String>();
                    Some(format!("[{}] {}...", msg.role, preview))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn estimate_tokens_saved(&self, messages: &[Message]) -> usize {
        // 估算节省的 token：原始的 70%
        let original: usize = messages.iter()
            .map(|m| m.metadata.token_count)
            .sum();

        (original as f64 * 0.7) as usize
    }
}

/// 自适应压缩路由器
pub struct AdaptiveCompressor {
    native: Arc<dyn CompressionProtocol>,
    fallbacks: Vec<Arc<dyn CompressionProtocol>>,
    metrics: Arc<CompressionMetrics>,
}

impl AdaptiveCompressor {
    pub fn new(
        native: Arc<dyn CompressionProtocol>,
        fallbacks: Vec<Arc<dyn CompressionProtocol>>,
    ) -> Self {
        Self {
            native,
            fallbacks,
            metrics: Arc::new(CompressionMetrics::new()),
        }
    }

    pub async fn compress(&self, context: &ConversationContext) -> CodexResult<CompressedContext> {
        let start = Instant::now();

        // 1. 尝试原生压缩
        match self.native.native_compress(context).await {
            Ok(result) => {
                self.metrics.record_success("native", start.elapsed());
                return Ok(result);
            }
            Err(e) if Self::is_transient_error(&e) => {
                tracing::warn!(
                    error = %e,
                    "native compression failed with transient error, trying fallback"
                );
            }
            Err(e) => {
                tracing::error!(
                    error = %e,
                    "native compression failed permanently"
                );
                self.metrics.record_failure("native");
            }
        }

        // 2. 尝试降级方案
        for (idx, fallback) in self.fallbacks.iter().enumerate() {
            match fallback.local_compress(context).await {
                Ok(result) => {
                    self.metrics.record_success(&format!("fallback_{}", idx), start.elapsed());
                    tracing::info!(
                        fallback_method = fallback.name(),
                        "compression succeeded with fallback"
                    );
                    return Ok(result);
                }
                Err(e) => {
                    tracing::warn!(
                        fallback_method = fallback.name(),
                        error = %e,
                        "fallback compression failed"
                    );
                    continue;
                }
            }
        }

        // 3. 所有方案都失败
        self.metrics.record_failure("all");
        Err(CodexErr::Stream(
            "All compression methods failed".to_string()
        ))
    }

    fn is_transient_error(error: &CodexErr) -> bool {
        // 判断是否是临时错误（网络、超时等）
        matches!(error, CodexErr::Stream(_))
    }
}

/// 压缩指标收集
pub struct CompressionMetrics {
    success_count: std::sync::atomic::AtomicU64,
    failure_count: std::sync::atomic::AtomicU64,
    total_latency_ms: std::sync::atomic::AtomicU64,
}

impl CompressionMetrics {
    pub fn new() -> Self {
        Self {
            success_count: std::sync::atomic::AtomicU64::new(0),
            failure_count: std::sync::atomic::AtomicU64::new(0),
            total_latency_ms: std::sync::atomic::AtomicU64::new(0),
        }
    }

    pub fn record_success(&self, method: &str, latency: Duration) {
        self.success_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.total_latency_ms.fetch_add(
            latency.as_millis() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );

        tracing::info!(
            method = method,
            latency_ms = latency.as_millis(),
            "compression succeeded"
        );
    }

    pub fn record_failure(&self, method: &str) {
        self.failure_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        tracing::error!(
            method = method,
            "compression failed"
        );
    }

    pub fn get_stats(&self) -> CompressionStats {
        let success = self.success_count.load(std::sync::atomic::Ordering::Relaxed);
        let failure = self.failure_count.load(std::sync::atomic::Ordering::Relaxed);
        let total_latency = self.total_latency_ms.load(std::sync::atomic::Ordering::Relaxed);

        CompressionStats {
            success_count: success,
            failure_count: failure,
            success_rate: if success + failure > 0 {
                success as f64 / (success + failure) as f64
            } else {
                0.0
            },
            avg_latency_ms: if success > 0 {
                total_latency / success
            } else {
                0
            },
        }
    }
}

#[derive(Debug)]
pub struct CompressionStats {
    pub success_count: u64,
    pub failure_count: u64,
    pub success_rate: f64,
    pub avg_latency_ms: u64,
}

// Anthropic Client 的简化接口（需要实际实现）
pub struct AnthropicClient {
    api_key: String,
    base_url: String,
}

impl AnthropicClient {
    pub async fn compress(
        &self,
        model: &str,
        system_prompt: &str,
        content: &str,
        max_tokens: usize,
    ) -> CodexResult<String> {
        // TODO: 实现实际的 API 调用
        // 这里只是一个占位符
        Err(CodexErr::Stream("Not implemented".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_local_compression() {
        let compressor = LocalHeuristicCompression::new();

        let context = ConversationContext {
            messages: vec![
                Message {
                    role: "user".to_string(),
                    content: "Hello".to_string(),
                    metadata: MessageMetadata {
                        has_code: false,
                        has_error: false,
                        is_directive: true,
                        token_count: 10,
                    },
                },
            ],
            total_tokens: 100,
            user_id: "test_user".to_string(),
        };

        let result = compressor.compress(&context).await.unwrap();
        assert_eq!(result.method, CompressionMethod::Local);
    }
}
