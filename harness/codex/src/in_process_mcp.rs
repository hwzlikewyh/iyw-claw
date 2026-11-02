//! 应用侧边界只使用标准 Future、字符串和 Tokio 字节流，不暴露上游 MCP 类型。

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub type McpOpenFuture =
    Pin<Box<dyn Future<Output = std::io::Result<tokio::io::DuplexStream>> + Send>>;

#[derive(Clone)]
pub struct BuiltinMcpTransport(pub(crate) codex_exec_server::InProcessMcpBridge);

impl BuiltinMcpTransport {
    pub fn new(
        endpoint: String,
        open: impl Fn(String, HashMap<String, String>) -> McpOpenFuture + Send + Sync + 'static,
    ) -> Self {
        Self(codex_exec_server::InProcessMcpBridge::new(
            endpoint,
            Arc::new(open),
        ))
    }
}
