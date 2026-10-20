use std::time::Duration;

use iyw_codex_harness::{
    desktop_xinghe_capabilities, HarnessConfig, UpstreamClient, UpstreamEvent, UpstreamEventPoll,
    UpstreamStartArgs,
};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::app_error::AppCommandError;

pub(crate) fn start_args(id: &str, spec: &Value) -> Result<UpstreamStartArgs, AppCommandError> {
    let storage = crate::acp::agent_storage::AgentStoragePaths::active()
        .ok_or_else(|| AppCommandError::configuration_invalid("智能体存储不可用"))?;
    let home = storage.profile(crate::models::AgentType::Codex).root;
    let helper = crate::internal_xinghe_worker::resolve_helper_path()
        .map_err(AppCommandError::configuration_invalid)?;
    let entry = super::mcp::canonical_to_codex_entry(spec)?;
    let mut servers = serde_json::Map::new();
    servers.insert(
        id.to_owned(),
        serde_json::to_value(entry)
            .map_err(|_| AppCommandError::configuration_invalid("连接器配置无效"))?,
    );
    Ok(UpstreamStartArgs {
        harness: HarnessConfig {
            experimental_api: true,
            ..Default::default()
        },
        runtime_fingerprint: "connector-oauth".into(),
        capabilities: desktop_xinghe_capabilities(),
        codex_home: home.clone(),
        cwd: home.clone(),
        workspace_roots: vec![home],
        helper_executable: helper.clone(),
        linux_sandbox_executable: cfg!(target_os = "linux").then_some(helper),
        main_execve_wrapper_executable: None,
        enable_codex_api_key_env: false,
        api_key: None,
        config_json: Some(json!({"mcp_servers":servers}).to_string()),
        runtime_environment: std::env::vars()
            .filter(|(key, _)| {
                !matches!(
                    key.as_str(),
                    "CODEX_API_KEY" | "OPENAI_API_KEY" | "CODEX_CONFIG"
                )
            })
            .collect(),
        mcp_server_openai_form_elicitation: false,
        opt_out_notification_methods: Vec::new(),
    })
}

pub(crate) async fn run(args: UpstreamStartArgs, context: (String, String, CancellationToken)) {
    let (attempt, name, cancel) = context;
    let started = tokio::time::timeout(Duration::from_secs(45), UpstreamClient::start(args)).await;
    let client = match started {
        Ok(Ok(client)) => client,
        _ => {
            tracing::warn!(connector=%name, "[MCP] OAuth 授权运行时启动失败或超时");
            super::mcp_oauth::update(&attempt, "failed", None);
            return;
        }
    };
    let result = tokio::select! {
        _ = cancel.cancelled() => "cancelled",
        outcome = tokio::time::timeout(super::mcp_oauth::LIFETIME, login(&client, (&attempt, &name))) => match outcome {
            Ok(Ok(true)) => "completed",
            _ => "failed",
        }
    };
    super::mcp_oauth::update(&attempt, result, None);
    tracing::info!(connector=%name, state=result, "[MCP] OAuth 授权流程结束");
    if client.shutdown().await.is_err() {
        tracing::warn!(connector=%name, "[MCP] OAuth 运行时关闭失败");
    }
}

async fn login(client: &UpstreamClient, context: (&str, &str)) -> Result<bool, ()> {
    let (attempt, name) = context;
    let response = client
        .request_global_json(
            json!({"method":"mcpServer/oauth/login", "params":{"name":name,"timeoutSecs":300}}),
        )
        .await
        .map_err(|_| ())?;
    let url = response["authorizationUrl"].as_str().ok_or(())?;
    let parsed = reqwest::Url::parse(url).map_err(|_| ())?;
    if !matches!(parsed.scheme(), "https" | "http")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(());
    }
    super::mcp_oauth::update(attempt, "pending", Some(url.to_string()));
    loop {
        match client
            .poll_event(Duration::from_secs(1))
            .await
            .map_err(|_| ())?
        {
            UpstreamEventPoll::Closed => return Err(()),
            UpstreamEventPoll::Event(event) => {
                if let UpstreamEvent::ServerNotification { method, params } = *event {
                    if method == "mcpServer/oauthLogin/completed"
                        && params["name"].as_str() == Some(name)
                        && params.get("loginId") == response.get("loginId")
                    {
                        return Ok(params["success"].as_bool() == Some(true));
                    }
                }
            }
            UpstreamEventPoll::Timeout => {}
        }
    }
}
