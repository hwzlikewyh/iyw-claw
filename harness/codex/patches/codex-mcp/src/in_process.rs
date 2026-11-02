//! 只有当前环境管理器显式绑定的端点才能使用宿主内存通道。

use std::collections::HashMap;
use std::sync::Arc;

use codex_config::{McpServerConfig, McpServerTransportConfig};
use codex_exec_server::InProcessMcpBridge;
use codex_rmcp_client::InProcessTransportFactory;
use futures::future::BoxFuture;
use tokio::io::DuplexStream;

pub(crate) fn factory(
    bridge: Option<&InProcessMcpBridge>,
    server: &str,
    config: &McpServerConfig,
) -> Result<Option<Arc<dyn InProcessTransportFactory>>, String> {
    let McpServerTransportConfig::StreamableHttp {
        url,
        http_headers,
        env_http_headers,
        bearer_token_env_var,
        http_headers_helper,
    } = &config.transport
    else {
        return Ok(None);
    };
    let Some(bridge) = bridge.filter(|bridge| bridge.matches(url)) else {
        return Ok(None);
    };
    if !config.is_local_environment()
        || bearer_token_env_var.is_some()
        || http_headers_helper.is_some()
        || env_http_headers
            .as_ref()
            .is_some_and(|headers| !headers.is_empty())
    {
        return Err("in-process MCP requires host-provided local credentials".into());
    }
    Ok(Some(Arc::new(Factory {
        bridge: bridge.clone(),
        server: server.to_string(),
        headers: http_headers.clone().unwrap_or_default(),
    })))
}

struct Factory {
    bridge: InProcessMcpBridge,
    server: String,
    headers: HashMap<String, String>,
}

impl InProcessTransportFactory for Factory {
    fn open(&self) -> BoxFuture<'static, std::io::Result<DuplexStream>> {
        self.bridge.open(self.server.clone(), self.headers.clone())
    }
}
