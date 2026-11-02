//! 宿主显式注入的本地 MCP 通道；不从配置或任意 loopback 地址推导内部权限。

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use futures::future::BoxFuture;
use tokio::io::DuplexStream;

pub type InProcessMcpOpen = dyn Fn(String, HashMap<String, String>) -> BoxFuture<'static, std::io::Result<DuplexStream>>
    + Send
    + Sync;

#[derive(Clone)]
pub struct InProcessMcpBridge {
    endpoint: String,
    open: Arc<InProcessMcpOpen>,
}

impl InProcessMcpBridge {
    pub fn new(endpoint: String, open: Arc<InProcessMcpOpen>) -> Self {
        Self { endpoint, open }
    }

    pub fn matches(&self, endpoint: &str) -> bool {
        self.endpoint == endpoint
    }

    pub fn open(
        &self,
        server: String,
        headers: HashMap<String, String>,
    ) -> BoxFuture<'static, std::io::Result<DuplexStream>> {
        (self.open)(server, headers)
    }
}

impl fmt::Debug for InProcessMcpBridge {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("InProcessMcpBridge")
            .finish_non_exhaustive()
    }
}
