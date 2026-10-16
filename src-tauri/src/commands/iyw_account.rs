use sea_orm::DatabaseConnection;
use serde_json::json;
#[cfg(feature = "tauri-runtime")]
use tauri::State;

use crate::app_error::{AppCommandError, AppErrorCode};
use crate::db::service::app_metadata_service;
#[cfg(feature = "tauri-runtime")]
use crate::db::AppDatabase;
use crate::models::agent::AgentType;

mod auth;
mod auth_forms;
mod balance_cache;
mod image_captcha;
mod login_flow;
mod pending;
mod points;
mod profile;
mod session;
mod spaces;
mod token_file;
mod types;
mod wire;

pub(crate) use session::{
    iyw_account_memory_credentials_core, iyw_account_product_context_core,
    iyw_account_usage_credentials_core, workspace_switching,
    WorkspaceSwitchGuard,
};
use types::StoredSession;
pub use types::{
    IywAccountAuthRequest, IywAccountProfile, IywAccountToken, IywLoginResult,
    IywWechatPollingResult, IywWechatPollingStatus, IywWechatQrcode, IywWorkspace,
};
use wire::{http_client, normalize_asset_url};

pub(crate) fn iyw_account_switch_guard(
    action: &str,
) -> Result<Option<WorkspaceSwitchGuard>, AppCommandError> {
    if action == "switch_workspace" {
        Ok(Some(WorkspaceSwitchGuard::begin()?))
    } else {
        Ok(None)
    }
}

const ACCOUNT_BASE_URL: &str = "https://account.iyw.cn";
const FUSION_API_BASE_URL: &str = "https://gateway.iyw.cn/iyw-fusion-api";
const DEFAULT_AVATAR_URL: &str =
    "https://chdesign.oss-cn-shanghai.aliyuncs.com/static/avatar/default.png";

fn account_error(message: Option<String>, fallback: &str) -> AppCommandError {
    AppCommandError::authentication_failed(message.unwrap_or_else(|| fallback.to_string()))
}

pub async fn iyw_account_auth_core(
    conn: &DatabaseConnection,
    action: String,
    data: serde_json::Value,
) -> Result<IywLoginResult, AppCommandError> {
    auth::run(conn, action, data).await
}

pub(crate) async fn iyw_account_access_token_core(
    conn: &DatabaseConnection,
) -> Result<Option<crate::acp::account_credentials::AccountAccessToken>, AppCommandError> {
    let session = session::fresh_session(conn).await?;
    if session.workspace_pending {
        return Err(AppCommandError::permission_denied(
            "空间切换尚未确认，请刷新账号信息",
        ));
    }
    Ok(session.token.and_then(|token| {
        crate::acp::account_credentials::AccountAccessToken::new(token.access_token)
    }))
}

pub(crate) async fn iyw_account_ensure_can_consume_core(
    conn: &DatabaseConnection,
) -> Result<(), AppCommandError> {
    balance_cache::ensure_can_consume(conn).await
}

pub async fn iyw_account_list_models_core(
    conn: &DatabaseConnection,
    agent_type: Option<AgentType>,
) -> Result<serde_json::Value, AppCommandError> {
    let token = iyw_account_access_token_core(conn)
        .await?
        .ok_or_else(|| AppCommandError::authentication_failed("Sign in to iyw-claw first"))?;
    let mut request = http_client()?
        .get(crate::acp::provider_overlay::model_gateway_models_url())
        .header("token", token.expose())
        .header("Content-Type", "application/json");
    let scoped_request = if let Some(agent_type) = agent_type {
        let platform_id = crate::acp::version_center::platform_id(conn, agent_type).await?;
        request = request.query(&[("sdk_id", &platform_id)]);
        Some((agent_type, platform_id))
    } else {
        None
    };
    let response = request.send().await.map_err(|error| {
        AppCommandError::network("Failed to fetch gateway models").with_detail(error.to_string())
    })?;
    if !response.status().is_success() {
        return Err(AppCommandError::network("Gateway model request failed")
            .with_detail(response.status().to_string()));
    }
    let payload = response
        .json::<serde_json::Value>()
        .await
        .map_err(|error| {
            AppCommandError::network("Gateway model response was invalid")
                .with_detail(error.to_string())
        })?;
    let catalog_changed = match scoped_request.as_ref() {
        None => crate::acp::model_catalog::update_from_payload(&payload),
        Some((agent_type, platform_id)) => crate::acp::model_catalog::replace_scoped_from_payload(
            *agent_type,
            platform_id,
            &payload,
        ),
    };
    if catalog_changed || scoped_request.is_some() {
        let projection = match scoped_request {
            Some((agent_type, _)) => {
                crate::acp::provider_overlay::enforce_active_provider_overlay(agent_type)
            }
            None => crate::acp::provider_overlay::enforce_existing_active_provider_overlays(),
        };
        if let Err(error) = projection {
            tracing::warn!("[ModelCatalog] failed to update Agent model projections: {error}");
        }
    }
    Ok(payload)
}

pub async fn iyw_account_get_wechat_qrcode_core() -> Result<IywWechatQrcode, AppCommandError> {
    auth::qrcode().await
}

pub async fn iyw_account_poll_wechat_login_core(
    conn: &DatabaseConnection,
    qr_token: String,
) -> Result<IywWechatPollingResult, AppCommandError> {
    auth::legacy_poll(conn, qr_token).await
}

pub async fn iyw_account_login_with_password_core(
    conn: &DatabaseConnection,
    username: String,
    password: String,
) -> Result<IywAccountProfile, AppCommandError> {
    let result = auth::run(
        conn,
        "password".to_string(),
        json!({"username":username,"password":password}),
    )
    .await?;
    result
        .profile
        .ok_or_else(|| AppCommandError::invalid_input("登录需要完成身份或空间选择"))
}

async fn account_profile(
    conn: &DatabaseConnection,
    include_balance: bool,
) -> Result<IywAccountProfile, AppCommandError> {
    let _guard = session::operation_lock().lock().await;
    let session = session::fresh_session(conn).await?;
    if session.token.is_none() {
        return Ok(IywAccountProfile::default());
    }
    profile::fetch_profile_with_session(conn, &session, include_balance).await
}

pub async fn iyw_account_get_identity_profile_core(
    conn: &DatabaseConnection,
) -> Result<IywAccountProfile, AppCommandError> {
    account_profile(conn, false).await
}

pub async fn iyw_account_get_profile_core(
    conn: &DatabaseConnection,
) -> Result<IywAccountProfile, AppCommandError> {
    account_profile(conn, true).await
}

pub async fn iyw_account_logout_core(conn: &DatabaseConnection) -> Result<(), AppCommandError> {
    let _guard = session::operation_lock().lock().await;
    crate::acp::account_credentials::clear_existing_agent_credentials().map_err(|error| {
        AppCommandError::configuration_invalid("Failed to clear private Agent credentials")
            .with_detail(error)
    })?;
    pending::clear().await;
    session::save_session(conn, &StoredSession::default()).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn iyw_account_auth(
    request: IywAccountAuthRequest,
    db: State<'_, AppDatabase>,
    manager: State<'_, crate::acp::manager::ConnectionManager>,
    browser: State<'_, crate::browser::BrowserSessionManager>,
) -> Result<IywLoginResult, AppCommandError> {
    let _switch_guard = if request.action == "switch_workspace" {
        Some(session::WorkspaceSwitchGuard::begin()?)
    } else {
        None
    };
    if request.action == "switch_workspace" && manager.has_active_account_turns().await {
        return Err(AppCommandError::permission_denied(
            "请先完成或停止正在运行的任务，再切换空间",
        ));
    }
    let result = iyw_account_auth_core(&db.conn, request.action, request.data).await?;
    if result.profile.is_some() {
        browser.refresh_iyw_browser_login().await;
    }
    Ok(result)
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn iyw_account_get_wechat_qrcode() -> Result<IywWechatQrcode, AppCommandError> {
    iyw_account_get_wechat_qrcode_core().await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn iyw_account_poll_wechat_login(
    qr_token: String,
    db: State<'_, AppDatabase>,
    browser: State<'_, crate::browser::BrowserSessionManager>,
) -> Result<IywWechatPollingResult, AppCommandError> {
    let result = iyw_account_poll_wechat_login_core(&db.conn, qr_token).await?;
    if result.profile.is_some() {
        browser.refresh_iyw_browser_login().await;
    }
    Ok(result)
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn iyw_account_login_with_password(
    username: String,
    password: String,
    db: State<'_, AppDatabase>,
    browser: State<'_, crate::browser::BrowserSessionManager>,
) -> Result<IywAccountProfile, AppCommandError> {
    let profile = iyw_account_login_with_password_core(&db.conn, username, password).await?;
    browser.refresh_iyw_browser_login().await;
    Ok(profile)
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn iyw_account_get_profile(
    db: State<'_, AppDatabase>,
) -> Result<IywAccountProfile, AppCommandError> {
    iyw_account_get_profile_core(&db.conn).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn iyw_account_list_models(
    agent_type: Option<AgentType>,
    db: State<'_, AppDatabase>,
    manager: State<'_, crate::acp::manager::ConnectionManager>,
) -> Result<serde_json::Value, AppCommandError> {
    let payload = iyw_account_list_models_core(&db.conn, agent_type).await?;
    manager.refresh_model_catalog_staleness(&db).await;
    Ok(payload)
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn iyw_account_logout(
    db: State<'_, AppDatabase>,
    browser: State<'_, crate::browser::BrowserSessionManager>,
) -> Result<(), AppCommandError> {
    iyw_account_logout_core(&db.conn).await?;
    browser.refresh_iyw_browser_login().await;
    Ok(())
}
