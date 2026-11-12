use std::time::Duration;

use iyw_codex_harness::UpstreamClient;
use sea_orm::DatabaseConnection;
use serde::Serialize;
use serde_json::{json, Value};

use crate::app_error::AppCommandError;

static CHECKS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
const START_TIMEOUT: Duration = Duration::from_secs(20);
const PROBE_TIMEOUT: Duration = Duration::from_secs(25);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(5);
const PROBE_POLL: Duration = Duration::from_millis(250);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorCheck {
    pub state: String,
    pub message: String,
    pub tool_count: Option<usize>,
}

pub(crate) fn result(state: &str, message: &str, tools: Option<usize>) -> ConnectorCheck {
    ConnectorCheck {
        state: state.into(),
        message: message.into(),
        tool_count: tools,
    }
}

pub(crate) async fn snapshot(
    conn: &DatabaseConnection,
    id: &str,
) -> Result<Value, AppCommandError> {
    let catalog =
        super::mcp_catalog::load_or_import(conn, super::mcp::scan_legacy_server_specs).await?;
    let entry = catalog
        .servers
        .get(id)
        .ok_or_else(|| AppCommandError::not_found("连接器已卸载"))?;
    super::mcp_catalog_sources::require_platform(entry)?;
    super::mcp_cli_config::require_scope(conn, &entry.spec).await?;
    if !entry.missing_config.is_empty() {
        return Err(AppCommandError::configuration_invalid("请先补充连接器配置"));
    }
    Ok(entry.spec.clone())
}

pub(crate) async fn require_current(
    conn: &DatabaseConnection,
    input: (&str, &Value, &str),
) -> Result<(), AppCommandError> {
    let (id, spec, owner) = input;
    if fingerprint(conn).await? != owner || snapshot(conn, id).await? != *spec {
        return Err(AppCommandError::invalid_input(
            "检查期间账号或连接器配置发生变化，请重新检查",
        ));
    }
    Ok(())
}

pub(crate) async fn fingerprint(conn: &DatabaseConnection) -> Result<String, AppCommandError> {
    let owner = super::mcp_cli_config::owner()?;
    let scope = super::mcp_cli_config::account_scope(conn).await?;
    Ok(format!("{owner}:{scope}"))
}

pub async fn check(conn: &DatabaseConnection, id: &str) -> Result<ConnectorCheck, AppCommandError> {
    let _permit = CHECKS
        .try_acquire()
        .map_err(|_| AppCommandError::invalid_input("连接检查正在进行，请稍后重试"))?;
    let _storage = crate::acp::agent_storage_work::begin_agent_storage_read().await;
    let owner = fingerprint(conn).await?;
    let spec = snapshot(conn, id).await?;
    let view = inspect(&spec, id).await?;
    require_current(conn, (id, &spec, &owner)).await?;
    tracing::info!(connector=%id, state=%view.state, "[MCP] 连接检查结束");
    Ok(view)
}

async fn inspect(spec: &Value, id: &str) -> Result<ConnectorCheck, AppCommandError> {
    if spec["iyw_market"]["auth_mode"].as_str() == Some("cli")
        && !super::mcp_cli_config::authenticated(spec).await?
    {
        return Ok(result(
            "authentication_required",
            "CLI 认证未通过，请完成登录或重新连接账号",
            None,
        ));
    }
    if !crate::internal_xinghe_worker::is_desktop_agent(crate::models::AgentType::Codex)
        || spec["type"].as_str() == Some("sse")
    {
        return Ok(result(
            "unverified",
            "当前环境无法独立检查 MCP；使用时由所选会话运行时验证",
            None,
        ));
    }
    let resolved = super::mcp::hydrate_market_spec(spec)?;
    let args = super::mcp_oauth_runtime::start_args(id, &resolved)?;
    let client = tokio::time::timeout(START_TIMEOUT, UpstreamClient::start(args))
        .await
        .map_err(|_| AppCommandError::task_execution_failed("连接检查运行时启动超时"))?
        .map_err(|_| AppCommandError::task_execution_failed("连接检查运行时无法启动"))?;
    let outcome = tokio::time::timeout(PROBE_TIMEOUT, probe(&client, id)).await;
    if tokio::time::timeout(CLOSE_TIMEOUT, client.shutdown())
        .await
        .is_err()
    {
        tracing::warn!(connector=%id, "[MCP] 连接检查运行时关闭超时");
    }
    outcome.map_err(|_| {
        AppCommandError::task_execution_failed("连接检查超时，请检查网络或服务启动情况")
    })?
}

async fn probe(client: &UpstreamClient, id: &str) -> Result<ConnectorCheck, AppCommandError> {
    let owner = iyw_codex_harness::SessionOwner::new(
        format!("connector-check-{}", uuid::Uuid::new_v4()),
        None,
        0,
    )
    .map_err(|_| AppCommandError::configuration_invalid("连接检查会话无效"))?;
    let started = client
        .start_thread(
            owner,
            json!({"method":"thread/start", "params":{"ephemeral":true}}),
            iyw_codex_harness::desktop_xinghe_capabilities(),
        )
        .await
        .map_err(|_| AppCommandError::task_execution_failed("无法建立临时连接检查会话"))?;
    let thread = started["thread"]["id"]
        .as_str()
        .ok_or_else(|| AppCommandError::task_execution_failed("连接检查会话标识缺失"))?;
    loop {
        let response = client
            .request_global_json(json!({"method":"mcpServerStatus/list",
            "params":{"threadId":thread,"serverName":id,"detail":"toolsAndAuthOnly"}}))
            .await
            .map_err(|_| AppCommandError::task_execution_failed("MCP 初始化或工具发现失败"))?;
        if let Some(view) = evaluate(&response, id)? {
            return Ok(view);
        }
        tokio::time::sleep(PROBE_POLL).await;
    }
}

fn evaluate(response: &Value, id: &str) -> Result<Option<ConnectorCheck>, AppCommandError> {
    let server = response["data"]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["name"] == id))
        .ok_or_else(|| AppCommandError::task_execution_failed("运行时未返回此连接器的状态"))?;
    if server["authStatus"].as_str() == Some("notLoggedIn")
        || server["runtimeStatus"].as_str() == Some("authenticationRequired")
    {
        return Ok(Some(result(
            "authentication_required",
            "连接器需要登录或授权已失效",
            None,
        )));
    }
    if matches!(
        server["runtimeStatus"].as_str(),
        Some("failed" | "cancelled" | "disabled")
    ) {
        return Ok(Some(result(
            "failed",
            "MCP 初始化失败，请检查服务及认证配置",
            None,
        )));
    }
    if server["runtimeStatus"].as_str() != Some("connected") {
        return Ok(None);
    }
    if server["toolsError"].as_str().is_some() || !server["serverInfo"].is_object() {
        return Ok(Some(result(
            "failed",
            "MCP 初始化或工具发现失败，请检查服务及认证配置",
            None,
        )));
    }
    let tools = server["tools"]
        .as_object()
        .ok_or_else(|| AppCommandError::task_execution_failed("MCP 工具目录格式无效"))?;
    if tools.values().any(|tool| {
        tool["name"]
            .as_str()
            .is_none_or(|name| name.trim().is_empty())
            || !tool["inputSchema"].is_object()
    }) {
        return Ok(Some(result(
            "failed",
            "MCP 工具定义不完整，请联系发布者",
            None,
        )));
    }
    Ok(Some(result(
        "ready",
        "MCP 初始化与工具发现成功；会话重连后应用最新状态",
        Some(tools.len()),
    )))
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn mcp_connector_check(
    server_id: String,
    db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<ConnectorCheck, AppCommandError> {
    check(&db.conn, &server_id).await
}
