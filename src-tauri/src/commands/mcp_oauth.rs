use std::collections::BTreeMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use sea_orm::DatabaseConnection;
use serde::Serialize;
use tokio_util::sync::CancellationToken;

use crate::app_error::AppCommandError;

const MAX_ATTEMPTS: usize = 8;
pub(crate) const LIFETIME: Duration = Duration::from_secs(300);
static ATTEMPTS: LazyLock<Mutex<BTreeMap<String, Attempt>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

#[derive(Clone, Serialize)]
pub struct OAuthStatus {
    pub id: String,
    pub state: String,
    pub authorization_url: Option<String>,
    pub error: Option<String>,
}

struct Attempt {
    view: OAuthStatus,
    created: Instant,
    cancel: CancellationToken,
}

pub async fn begin(conn: &DatabaseConnection, id: String) -> Result<OAuthStatus, AppCommandError> {
    if !crate::internal_xinghe_worker::is_desktop_agent(crate::models::AgentType::Codex) {
        return Err(AppCommandError::configuration_invalid(
            "MCP OAuth 授权目前需要桌面内置星河",
        ));
    }
    let storage_guard = crate::acp::agent_storage_work::begin_agent_storage_read().await;
    super::mcp::require_private_agent_storage_for_write()?;
    let catalog =
        super::mcp_catalog::load_or_import(conn, super::mcp::scan_legacy_server_specs).await?;
    let entry = catalog
        .servers
        .get(&id)
        .ok_or_else(|| AppCommandError::not_found("连接器未安装"))?;
    if entry.spec["iyw_market"]["auth_mode"].as_str() != Some("oauth")
        || entry.spec["type"].as_str() != Some("http")
    {
        return Err(AppCommandError::invalid_input(
            "此连接器未声明 Streamable HTTP OAuth",
        ));
    }
    let spec = super::mcp::hydrate_market_spec(&entry.spec)?;
    let args = super::mcp_oauth_runtime::start_args(&id, &spec)?;
    let (view, cancel) = reserve()?;
    let attempt_id = view.id.clone();
    tokio::spawn(async move {
        let _storage_guard = storage_guard;
        super::mcp_oauth_runtime::run(args, (attempt_id, id, cancel)).await;
    });
    Ok(view)
}

fn reserve() -> Result<(OAuthStatus, CancellationToken), AppCommandError> {
    let mut attempts = ATTEMPTS
        .lock()
        .map_err(|_| AppCommandError::configuration_invalid("授权状态不可用"))?;
    attempts.retain(|_, attempt| attempt.created.elapsed() < LIFETIME + Duration::from_secs(60));
    if attempts.len() >= MAX_ATTEMPTS
        || attempts
            .values()
            .any(|item| matches!(item.view.state.as_str(), "starting" | "pending"))
    {
        return Err(AppCommandError::invalid_input(
            "请先完成或取消当前连接器授权",
        ));
    }
    let view = OAuthStatus {
        id: uuid::Uuid::new_v4().to_string(),
        state: "starting".into(),
        authorization_url: None,
        error: None,
    };
    let cancel = CancellationToken::new();
    attempts.insert(
        view.id.clone(),
        Attempt {
            view: view.clone(),
            created: Instant::now(),
            cancel: cancel.clone(),
        },
    );
    Ok((view, cancel))
}

pub fn status(id: &str) -> Result<OAuthStatus, AppCommandError> {
    ATTEMPTS
        .lock()
        .ok()
        .and_then(|attempts| attempts.get(id).map(|item| item.view.clone()))
        .ok_or_else(|| AppCommandError::not_found("授权请求已过期，请重新授权"))
}

pub fn cancel(id: &str) -> Result<OAuthStatus, AppCommandError> {
    let mut attempts = ATTEMPTS
        .lock()
        .map_err(|_| AppCommandError::configuration_invalid("授权状态不可用"))?;
    let attempt = attempts
        .get_mut(id)
        .ok_or_else(|| AppCommandError::not_found("授权请求已过期"))?;
    if matches!(attempt.view.state.as_str(), "starting" | "pending") {
        attempt.cancel.cancel();
        attempt.view.state = "cancelled".into();
        attempt.view.authorization_url = None;
    }
    Ok(attempt.view.clone())
}

pub(crate) fn update(id: &str, state: &str, url: Option<String>) {
    if let Ok(mut attempts) = ATTEMPTS.lock() {
        if let Some(attempt) = attempts.get_mut(id) {
            if attempt.cancel.is_cancelled() {
                return;
            }
            attempt.view.state = state.to_owned();
            attempt.view.authorization_url = url;
            attempt.view.error =
                (state == "failed").then(|| "授权未完成，请检查服务端 OAuth 设置后重试".into());
        }
    }
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn mcp_oauth_begin(
    server_id: String,
    db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<OAuthStatus, AppCommandError> {
    begin(&db.conn, server_id).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn mcp_oauth_status(attempt_id: String) -> Result<OAuthStatus, AppCommandError> {
    status(&attempt_id)
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn mcp_oauth_cancel(attempt_id: String) -> Result<OAuthStatus, AppCommandError> {
    cancel(&attempt_id)
}
