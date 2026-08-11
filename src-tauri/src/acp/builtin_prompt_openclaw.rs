use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use serde_json::{json, Value};

use crate::acp::agent_storage::AgentStoragePaths;
use crate::acp::builtin_agent_prompt::RenderedBuiltinPrompt;
use crate::acp::error::AcpError;
use crate::models::agent::AgentType;

const AGENT_ID: &str = "iyw-claw";
const AGENT_WORKSPACE: &str = "~/.openclaw/workspace-iyw-claw";
const CLI_TIMEOUT_MS: &str = "15000";
const PROCESS_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Default)]
pub struct OpenClawPromptRoute {
    pub session_key: Option<String>,
}

pub struct PrepareRequest<'a> {
    pub storage: &'a AgentStoragePaths,
    pub environment: &'a BTreeMap<String, String>,
    pub prompt: &'a RenderedBuiltinPrompt,
    pub session_id: Option<&'a str>,
}

pub async fn prepare(request: PrepareRequest<'_>) -> Result<OpenClawPromptRoute, AcpError> {
    let client = GatewayClient::new(request.storage, request.environment)?;
    ensure_agent(&client).await?;
    client
        .call(
            "agents.files.set",
            json!({
                "agentId": AGENT_ID,
                "name": "AGENTS.md",
                "content": request.prompt.text.as_ref(),
            }),
        )
        .await?;
    let session_key = request.session_id.map(session_key);
    tracing::info!(
        prompt_hash = %request.prompt.hash,
        resumed = request.session_id.is_some(),
        "[ACP] OpenClaw built-in prompt prepared"
    );
    Ok(OpenClawPromptRoute { session_key })
}

pub fn session_key(session_id: &str) -> String {
    format!("agent:{AGENT_ID}:acp-bridge:{session_id}")
}

async fn ensure_agent(client: &GatewayClient<'_>) -> Result<(), AcpError> {
    if agent_exists(client).await? {
        return Ok(());
    }
    let created = client
        .call(
            "agents.create",
            json!({"name": AGENT_ID, "workspace": AGENT_WORKSPACE}),
        )
        .await;
    if let Err(create_error) = created {
        if agent_exists(client).await.unwrap_or(false) {
            return Ok(());
        }
        return Err(create_error);
    }
    Ok(())
}

async fn agent_exists(client: &GatewayClient<'_>) -> Result<bool, AcpError> {
    let result = client.call("agents.list", json!({})).await?;
    Ok(result
        .get("agents")
        .and_then(Value::as_array)
        .is_some_and(|agents| {
            agents
                .iter()
                .any(|agent| agent.get("id").and_then(Value::as_str) == Some(AGENT_ID))
        }))
}

struct GatewayClient<'a> {
    command: PathBuf,
    environment: &'a BTreeMap<String, String>,
}

impl<'a> GatewayClient<'a> {
    fn new(
        storage: &AgentStoragePaths,
        environment: &'a BTreeMap<String, String>,
    ) -> Result<Self, AcpError> {
        let version = environment
            .get(crate::commands::acp::MANAGED_AGENT_VERSION_ENV)
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| injection_error("OpenClaw managed version is missing"))?;
        let command = crate::acp::npm_runtime::resolve_private_npm_command(
            storage,
            AgentType::OpenClaw,
            version,
            "openclaw",
        )
        .ok_or_else(|| injection_error("OpenClaw private command is missing"))?;
        Ok(Self {
            command,
            environment,
        })
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value, AcpError> {
        let params = serde_json::to_string(&params)
            .map_err(|error| injection_error(format!("failed to encode {method}: {error}")))?;
        let mut command = crate::process::tokio_command(&self.command);
        command
            .args(["gateway", "call", method, "--params", &params])
            .args(["--timeout", CLI_TIMEOUT_MS, "--json"])
            .envs(self.environment)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(url) = self
            .environment
            .get("OPENCLAW_GATEWAY_URL")
            .filter(|value| !value.trim().is_empty())
        {
            command.args(["--url", url]);
        }
        run_gateway_command(command, method).await
    }
}

async fn run_gateway_command(
    mut command: tokio::process::Command,
    method: &str,
) -> Result<Value, AcpError> {
    let child = command
        .spawn()
        .map_err(|error| injection_error(format!("failed to start OpenClaw RPC: {error}")))?;
    let pid = child.id();
    let output = match tokio::time::timeout(PROCESS_TIMEOUT, child.wait_with_output()).await {
        Ok(output) => output
            .map_err(|error| injection_error(format!("failed to wait for {method}: {error}")))?,
        Err(_) => {
            if let Some(pid) = pid {
                let _ = kill_tree::tokio::kill_tree(pid).await;
            }
            return Err(injection_error(format!("OpenClaw RPC {method} timed out")));
        }
    };
    let parsed = parse_gateway_json(&output.stdout, method)?;
    if output.status.success() {
        return Ok(parsed);
    }
    let detail = if method == "agents.files.set" {
        String::new()
    } else {
        gateway_error_detail(&parsed)
    };
    Err(injection_error(format!(
        "OpenClaw RPC {method} failed with status {}{detail}",
        output.status
    )))
}

fn parse_gateway_json(raw: &[u8], method: &str) -> Result<Value, AcpError> {
    serde_json::from_slice(raw).map_err(|error| {
        injection_error(format!(
            "OpenClaw RPC {method} returned invalid JSON ({} bytes): {error}",
            raw.len()
        ))
    })
}

fn gateway_error_detail(value: &Value) -> String {
    value
        .pointer("/error/message")
        .or_else(|| value.get("message"))
        .and_then(Value::as_str)
        .map(|message| {
            let clean = message.replace(['\r', '\n'], " ");
            format!(": {}", clean.chars().take(512).collect::<String>())
        })
        .unwrap_or_default()
}

fn injection_error(message: impl Into<String>) -> AcpError {
    AcpError::BuiltinPromptInjection(message.into())
}
