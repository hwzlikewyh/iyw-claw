mod commit;
mod providers;
mod session;
mod types;

pub use types::{QrPollResponse, QrStartResponse, QrStatus};

use crate::app_error::AppCommandError;
use crate::chat_channel::manager::ChatChannelManager;
use crate::chat_channel::types::ChannelType;
use crate::db::service::chat_channel_service;
use crate::db::AppDatabase;

use self::session::Session;
use self::types::{LarkRegion, ProviderCredentials, ProviderPoll};

const MAX_VERIFY_CODE_CHARS: usize = 64;
const MAX_CONSECUTIVE_POLL_FAILURES: u8 = 3;

pub async fn start(
    db: &AppDatabase,
    channel_id: i32,
    requested_type: Option<&str>,
    variant: Option<&str>,
) -> Result<QrStartResponse, AppCommandError> {
    let _channel_guard = crate::chat_channel::operation_lock::lock_channel(channel_id).await;
    let (channel_type, stored_type) = load_channel_type(db, channel_id).await?;
    validate_start_type(channel_type, &stored_type, requested_type)?;
    session::prepare(channel_id).await;

    let local_tokens = if channel_type == ChannelType::Weixin {
        recent_weixin_tokens(db).await?
    } else {
        Vec::new()
    };
    let provider = providers::start(channel_type, LarkRegion::parse(variant), &local_tokens)
        .await
        .map_err(AppCommandError::from)?;
    let response = session::insert(channel_id, channel_type, provider).await?;
    tracing::info!(
        channel_id,
        channel_type = %stored_type,
        stage = "qr_started",
        "[ChatChannel] QR onboarding session started"
    );
    Ok(response)
}

pub async fn poll(
    db: &AppDatabase,
    manager: &ChatChannelManager,
    session_id: &str,
    verify_code: Option<&str>,
) -> Result<QrPollResponse, AppCommandError> {
    let active = session::find(session_id).await?;
    let _poll_guard = active.poll_lock.lock().await;
    if let Some(response) = session::preflight(&active).await {
        return Ok(response);
    }
    let verify_code = validate_verify_code(verify_code)?;
    let result = providers::poll(&active.provider, verify_code).await;
    match result {
        Ok(provider_poll) => {
            session::reset_poll_failures(&active);
            apply_provider_poll(db, manager, &active, provider_poll).await
        }
        Err(error) => {
            let failures = session::record_poll_failure(&active);
            tracing::warn!(
                channel_id = active.channel_id,
                channel_type = %active.channel_type,
                error_category = error.category(),
                consecutive_failures = failures,
                stage = "qr_poll",
                "[ChatChannel] QR provider polling failed"
            );
            if failures < MAX_CONSECUTIVE_POLL_FAILURES {
                return Ok(session::keep_waiting(&active).await);
            }
            Ok(session::set_terminal(&active, QrStatus::Error, Some("provider_poll_failed")).await)
        }
    }
}

pub async fn cancel(session_id: &str) -> Result<QrPollResponse, AppCommandError> {
    session::cancel(session_id).await
}

async fn apply_provider_poll(
    db: &AppDatabase,
    manager: &ChatChannelManager,
    active: &Session,
    result: ProviderPoll,
) -> Result<QrPollResponse, AppCommandError> {
    let response = match result {
        ProviderPoll::Waiting => session::keep_waiting(active).await,
        ProviderPoll::Scanned => session::set_status(active, QrStatus::Scanned, None).await,
        ProviderPoll::VerificationRequired => {
            session::set_status(active, QrStatus::Scanned, Some("verify_code_required")).await
        }
        ProviderPoll::Expired => {
            session::set_terminal(active, QrStatus::Expired, Some("expired")).await
        }
        ProviderPoll::Denied(code) => {
            session::set_terminal(active, QrStatus::Denied, Some(code)).await
        }
        ProviderPoll::Approved(credentials) => {
            return commit_approved(db, manager, active, credentials).await;
        }
    };
    Ok(response)
}

async fn commit_approved(
    db: &AppDatabase,
    manager: &ChatChannelManager,
    active: &Session,
    credentials: ProviderCredentials,
) -> Result<QrPollResponse, AppCommandError> {
    session::set_status(active, QrStatus::Connecting, None).await;
    match commit::commit_credentials(db, manager, active, credentials).await {
        Ok(commit::CommitOutcome::Connected) => {
            tracing::info!(
                channel_id = active.channel_id,
                channel_type = %active.channel_type,
                stage = "qr_connected",
                "[ChatChannel] QR onboarding runtime connected"
            );
            Ok(session::set_terminal(active, QrStatus::Connected, None).await)
        }
        Ok(commit::CommitOutcome::Cancelled) => {
            Ok(session::set_terminal(active, QrStatus::Cancelled, None).await)
        }
        Err(error) => {
            tracing::error!(
                channel_id = active.channel_id,
                channel_type = %active.channel_type,
                error = %error,
                stage = "qr_commit",
                "[ChatChannel] QR onboarding credential commit failed"
            );
            Ok(
                session::set_terminal(active, QrStatus::Error, Some("credential_commit_failed"))
                    .await,
            )
        }
    }
}

async fn load_channel_type(
    db: &AppDatabase,
    channel_id: i32,
) -> Result<(ChannelType, String), AppCommandError> {
    let model = chat_channel_service::get_by_id(&db.conn, channel_id)
        .await
        .map_err(AppCommandError::from)?
        .ok_or_else(|| {
            AppCommandError::not_found(format!("Chat channel {channel_id} not found"))
        })?;
    let channel_type = parse_channel_type(&model.channel_type)?;
    Ok((channel_type, model.channel_type))
}

fn validate_start_type(
    channel_type: ChannelType,
    stored_type: &str,
    requested_type: Option<&str>,
) -> Result<(), AppCommandError> {
    if requested_type.is_some_and(|requested| requested != stored_type) {
        return Err(AppCommandError::invalid_input(
            "扫码渠道类型与已保存渠道不一致",
        ));
    }
    if !matches!(
        channel_type,
        ChannelType::Weixin | ChannelType::WecomAiBot | ChannelType::Dingtalk | ChannelType::Lark
    ) {
        return Err(AppCommandError::invalid_input("该渠道不支持扫码接入"));
    }
    Ok(())
}

fn parse_channel_type(value: &str) -> Result<ChannelType, AppCommandError> {
    serde_json::from_value(serde_json::Value::String(value.to_string()))
        .map_err(|_| AppCommandError::configuration_invalid(format!("未知渠道类型：{value}")))
}

fn validate_verify_code(value: Option<&str>) -> Result<Option<&str>, AppCommandError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if value.chars().count() > MAX_VERIFY_CODE_CHARS || value.chars().any(char::is_control) {
        return Err(AppCommandError::invalid_input("验证码格式无效"));
    }
    Ok(Some(value))
}

pub(crate) async fn recent_weixin_tokens(db: &AppDatabase) -> Result<Vec<String>, AppCommandError> {
    let rows = chat_channel_service::list_all(&db.conn)
        .await
        .map_err(AppCommandError::from)?;
    let mut tokens = Vec::new();
    for row in rows
        .into_iter()
        .rev()
        .filter(|row| row.channel_type == "weixin")
    {
        push_channel_token(&mut tokens, row.id);
        if tokens.len() == 10 {
            break;
        }
    }
    Ok(tokens)
}

fn push_channel_token(tokens: &mut Vec<String>, channel_id: i32) {
    match crate::keyring_store::try_get_channel_token(channel_id) {
        Ok(Some(token)) if !token.trim().is_empty() && !tokens.contains(&token) => {
            tokens.push(token);
        }
        Ok(_) => {}
        Err(error) => tracing::warn!(
            channel_id,
            error = %error,
            "[Weixin] ignored unreadable saved credential while creating QR code"
        ),
    }
}
