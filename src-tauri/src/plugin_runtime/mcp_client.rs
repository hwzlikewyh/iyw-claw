use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use rmcp::model::{
    CallToolRequestParams, CallToolResult, ReadResourceRequestParams, ReadResourceResult,
};
use rmcp::service::{Peer, RoleClient, RunningService};
use tokio::io::AsyncReadExt;
use tokio::process::Child;
use tokio::sync::Mutex;

use super::process;
use super::types::{PluginInvokeError, RuntimeLaunchSpec};

const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(15);
const CONTRACT_TIMEOUT: Duration = Duration::from_secs(15);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(4);
const STDERR_BUFFER_BYTES: usize = 4096;
const MAX_SCHEMA_BYTES: usize = 1 << 20;

pub(super) struct PluginMcpClient {
    peer: Peer<RoleClient>,
    service: Mutex<Option<RunningService<RoleClient, ()>>>,
    child: Mutex<Option<Child>>,
    stderr_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl PluginMcpClient {
    pub async fn start(spec: &RuntimeLaunchSpec) -> Result<Arc<Self>, PluginInvokeError> {
        let mut process = process::spawn(spec)?;
        let stderr_task = consume_stderr(process.stderr, spec.key.clone());
        let stdin = match process.child.stdin.take() {
            Some(stdin) => stdin,
            None => return cleanup_spawn_failure(process.child, stderr_task, "stdin"),
        };
        let stdout = match process.child.stdout.take() {
            Some(stdout) => stdout,
            None => return cleanup_spawn_failure(process.child, stderr_task, "stdout"),
        };
        let mut service = match tokio::time::timeout(
            INITIALIZE_TIMEOUT,
            rmcp::serve_client(
                (),
                rmcp::transport::AsyncRwTransport::new_client(stdout, stdin),
            ),
        )
        .await
        {
            Ok(Ok(service)) => service,
            Ok(Err(error)) => {
                terminate_child(&mut process.child).await;
                stderr_task.abort();
                return Err(PluginInvokeError::before_effect(
                    "plugin_initialize_failed",
                    error.to_string(),
                ));
            }
            Err(_) => {
                terminate_child(&mut process.child).await;
                stderr_task.abort();
                return Err(PluginInvokeError::before_effect(
                    "plugin_initialize_timeout",
                    "MCP initialize timed out",
                ));
            }
        };
        let contract =
            tokio::time::timeout(CONTRACT_TIMEOUT, validate_contract(service.peer(), spec))
                .await
                .unwrap_or_else(|_| {
                    Err(PluginInvokeError::before_effect(
                        "plugin_contract_timeout",
                        "MCP contract validation timed out",
                    ))
                });
        if let Err(error) = contract {
            let _ = service.close_with_timeout(CLOSE_TIMEOUT).await;
            terminate_child(&mut process.child).await;
            stderr_task.abort();
            return Err(error);
        }
        let peer = service.peer().clone();
        Ok(Arc::new(Self {
            peer,
            service: Mutex::new(Some(service)),
            child: Mutex::new(Some(process.child)),
            stderr_task: Mutex::new(Some(stderr_task)),
        }))
    }

    pub async fn call_tool(
        &self,
        name: String,
        arguments: serde_json::Map<String, serde_json::Value>,
    ) -> Result<CallToolResult, PluginInvokeError> {
        self.peer
            .call_tool(CallToolRequestParams::new(name).with_arguments(arguments))
            .await
            .map_err(|error| {
                PluginInvokeError::after_dispatch("plugin_call_failed", error.to_string())
            })
    }

    pub async fn read_resource(
        &self,
        uri: String,
    ) -> Result<ReadResourceResult, PluginInvokeError> {
        self.peer
            .read_resource(ReadResourceRequestParams::new(uri))
            .await
            .map_err(|error| {
                PluginInvokeError::after_dispatch("plugin_resource_read_failed", error.to_string())
            })
    }

    pub async fn is_closed(&self) -> bool {
        self.service
            .lock()
            .await
            .as_ref()
            .is_none_or(|service| service.is_closed())
    }

    pub async fn shutdown(&self) {
        if let Some(mut service) = self.service.lock().await.take() {
            let _ = service.close_with_timeout(CLOSE_TIMEOUT).await;
        }
        if let Some(mut child) = self.child.lock().await.take() {
            terminate_child(&mut child).await;
        }
        if let Some(task) = self.stderr_task.lock().await.take() {
            task.abort();
        }
    }
}

async fn validate_contract(
    peer: &Peer<RoleClient>,
    spec: &RuntimeLaunchSpec,
) -> Result<(), PluginInvokeError> {
    let tools = peer.list_all_tools().await.map_err(|error| {
        PluginInvokeError::before_effect("plugin_contract_unavailable", error.to_string())
    })?;
    let tools = tools
        .into_iter()
        .map(|tool| (tool.name.to_string(), tool))
        .collect::<BTreeMap<_, _>>();
    for expected in &spec.expected_tools {
        let tool = tools.get(&expected.name).ok_or_else(|| {
            PluginInvokeError::before_effect(
                "plugin_contract_mismatch",
                format!("missing tool {}", expected.name),
            )
        })?;
        let expected_schema = read_schema(spec, &expected.schema_path)?;
        let actual_schema = serde_json::Value::Object((*tool.input_schema).clone());
        if actual_schema != expected_schema {
            return Err(PluginInvokeError::before_effect(
                "plugin_contract_mismatch",
                format!("schema mismatch for {}", expected.name),
            ));
        }
    }
    if !spec.resource_uris.is_empty() {
        let resources = peer.list_all_resources().await.map_err(|error| {
            PluginInvokeError::before_effect("plugin_contract_unavailable", error.to_string())
        })?;
        for uri in &spec.resource_uris {
            if !resources.iter().any(|resource| resource.raw.uri == *uri) {
                return Err(PluginInvokeError::before_effect(
                    "plugin_contract_mismatch",
                    format!("missing resource {uri}"),
                ));
            }
        }
    }
    Ok(())
}

fn read_schema(
    spec: &RuntimeLaunchSpec,
    relative: &str,
) -> Result<serde_json::Value, PluginInvokeError> {
    let root = std::fs::canonicalize(&spec.install_root).map_err(|error| {
        PluginInvokeError::before_effect("plugin_contract_mismatch", error.to_string())
    })?;
    let path = std::fs::canonicalize(root.join(relative)).map_err(|error| {
        PluginInvokeError::before_effect("plugin_contract_mismatch", error.to_string())
    })?;
    if !path.starts_with(&root) {
        return Err(PluginInvokeError::before_effect(
            "plugin_contract_mismatch",
            "schema path escapes plugin root",
        ));
    }
    let bytes = std::fs::read(path).map_err(|error| {
        PluginInvokeError::before_effect("plugin_contract_mismatch", error.to_string())
    })?;
    if bytes.len() > MAX_SCHEMA_BYTES {
        return Err(PluginInvokeError::before_effect(
            "plugin_contract_mismatch",
            "capability schema is oversized",
        ));
    }
    serde_json::from_slice(&bytes).map_err(|error| {
        PluginInvokeError::before_effect("plugin_contract_mismatch", error.to_string())
    })
}

fn consume_stderr(
    mut stderr: tokio::process::ChildStderr,
    key: super::types::RuntimeKey,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut buffer = [0_u8; STDERR_BUFFER_BYTES];
        let mut total = 0_u64;
        while let Ok(size) = stderr.read(&mut buffer).await {
            if size == 0 {
                break;
            }
            total = total.saturating_add(size as u64);
        }
        if total > 0 {
            tracing::warn!(
                plugin = %key.plugin_slug,
                connector = %key.connector_key,
                stderr_bytes = total,
                "[plugin-runtime] plugin wrote to stderr"
            );
        }
    })
}

async fn terminate_child(child: &mut Child) {
    if let Some(pid) = child.id() {
        let _ = kill_tree::tokio::kill_tree(pid).await;
    }
    let _ = child.wait().await;
}

fn cleanup_spawn_failure(
    mut child: Child,
    stderr_task: tokio::task::JoinHandle<()>,
    pipe: &str,
) -> Result<Arc<PluginMcpClient>, PluginInvokeError> {
    stderr_task.abort();
    tokio::spawn(async move { terminate_child(&mut child).await });
    Err(PluginInvokeError::before_effect(
        "plugin_start_failed",
        format!("{pipe} pipe is unavailable"),
    ))
}
