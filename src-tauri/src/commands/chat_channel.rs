use crate::app_error::AppCommandError;
use crate::chat_channel::backends::weixin::{WeixinQrcodeInfo, WeixinQrcodeStatusPublic};
use crate::chat_channel::manager::ChatChannelManager;
use crate::chat_channel::natural_router_config::{
    ChatNaturalRouterConfig, ChatNaturalRouterConfigInput,
};
use crate::chat_channel::types::ChannelType;
use crate::chat_channel::webhook::WebhookConfig;
use crate::db::service::{chat_channel_message_log_service, chat_channel_service};
use crate::db::AppDatabase;
use crate::models::chat_channel::{ChannelStatusInfo, ChatChannelInfo, ChatChannelMessageLogInfo};

// ---------------------------------------------------------------------------
// Shared core functions (used by both Tauri commands and web handlers)
// ---------------------------------------------------------------------------

pub async fn list_chat_channels_core(
    db: &AppDatabase,
) -> Result<Vec<ChatChannelInfo>, AppCommandError> {
    let rows = chat_channel_service::list_all(&db.conn)
        .await
        .map_err(AppCommandError::from)?;
    Ok(rows.into_iter().map(ChatChannelInfo::from).collect())
}

pub async fn create_chat_channel_core(
    db: &AppDatabase,
    name: String,
    channel_type: String,
    config_json: String,
    enabled: bool,
    daily_report_enabled: bool,
    daily_report_time: Option<String>,
) -> Result<ChatChannelInfo, AppCommandError> {
    // Validate channel_type
    let _: ChannelType = serde_json::from_value(serde_json::Value::String(channel_type.clone()))
        .map_err(|_| {
            AppCommandError::invalid_input(format!("Invalid channel type: {channel_type}"))
        })?;

    let model = chat_channel_service::create(
        &db.conn,
        name,
        channel_type,
        config_json,
        enabled,
        daily_report_enabled,
        daily_report_time,
    )
    .await
    .map_err(AppCommandError::from)?;

    // Auto-create a dedicated workspace folder for this channel so messages
    // route there without any heuristics — zero-config for the user.
    let info = init_channel_workspace(db, model).await;
    Ok(info)
}

/// Compute the channel's dedicated workspace root.
/// Daily subfolders (`{root}/{YYYY-MM-DD}/`) are created on demand by the
/// natural router — this function only returns the persistent root path.
pub fn channel_workspace_root(channel_id: i32) -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".iyw-claw")
        .join("channel-workspaces")
        .join(channel_id.to_string())
}

/// After channel creation, create the workspace root directory on disk and
/// store its path in the channel's config_json so the natural router can
/// create per-day subfolders without any heuristics.
/// All errors are non-fatal.
async fn init_channel_workspace(
    db: &AppDatabase,
    model: crate::db::entities::chat_channel::Model,
) -> ChatChannelInfo {
    let root = channel_workspace_root(model.id);
    if let Err(e) = std::fs::create_dir_all(&root) {
        tracing::warn!(
            "[create_chat_channel] workspace root creation failed for channel {}: {e}",
            model.id
        );
        return ChatChannelInfo::from(model);
    }

    // Store the root path; daily subfolders ({root}/{date}/) are created on
    // demand — no folder row is registered here.
    let updated_config =
        patch_config_with_workspace_root(&model.config_json, &root.to_string_lossy());
    match chat_channel_service::update(
        &db.conn,
        model.id,
        None,
        None,
        Some(updated_config),
        None,
        None,
        None,
    )
    .await
    {
        Ok(updated) => {
            tracing::info!(
                "[create_chat_channel] channel {} workspace root ready: {}",
                model.id,
                root.display()
            );
            ChatChannelInfo::from(updated)
        }
        Err(e) => {
            tracing::warn!(
                "[create_chat_channel] failed to persist workspace root for channel {}: {e}",
                model.id
            );
            ChatChannelInfo::from(model)
        }
    }
}

fn patch_config_with_workspace_root(config_json: &str, root_path: &str) -> String {
    let mut config: serde_json::Value =
        serde_json::from_str(config_json).unwrap_or_else(|_| serde_json::json!({}));
    if let serde_json::Value::Object(ref mut map) = config {
        map.insert(
            "channel_workspace_root".to_string(),
            serde_json::Value::String(root_path.to_string()),
        );
        // Remove any legacy default_folder_id written by older versions.
        map.remove("default_folder_id");
    }
    config.to_string()
}

#[allow(clippy::too_many_arguments)]
pub async fn update_chat_channel_core(
    db: &AppDatabase,
    id: i32,
    name: Option<String>,
    enabled: Option<bool>,
    config_json: Option<String>,
    event_filter_json: Option<Option<String>>,
    daily_report_enabled: Option<bool>,
    daily_report_time: Option<Option<String>>,
) -> Result<ChatChannelInfo, AppCommandError> {
    let model = chat_channel_service::update(
        &db.conn,
        id,
        name,
        enabled,
        config_json,
        event_filter_json,
        daily_report_enabled,
        daily_report_time,
    )
    .await
    .map_err(AppCommandError::from)?;
    Ok(ChatChannelInfo::from(model))
}

pub async fn delete_chat_channel_core(
    db: &AppDatabase,
    manager: &ChatChannelManager,
    id: i32,
) -> Result<(), AppCommandError> {
    // Disconnect running backend before deleting from DB (prevents orphaned task)
    let _ = manager.remove_channel(id).await;
    chat_channel_service::delete(&db.conn, id)
        .await
        .map_err(AppCommandError::from)?;
    let _ = crate::keyring_store::delete_channel_token(id);
    Ok(())
}

pub async fn connect_chat_channel_core(
    db: &AppDatabase,
    manager: &ChatChannelManager,
    id: i32,
) -> Result<(), AppCommandError> {
    let model = chat_channel_service::get_by_id(&db.conn, id)
        .await
        .map_err(AppCommandError::from)?
        .ok_or_else(|| AppCommandError::not_found(format!("Chat channel {id} not found")))?;

    let channel_type: ChannelType = serde_json::from_value(serde_json::Value::String(
        model.channel_type.clone(),
    ))
    .map_err(|_| {
        AppCommandError::configuration_invalid(format!(
            "Invalid channel type: {}",
            model.channel_type
        ))
    })?;

    let config: serde_json::Value = serde_json::from_str(&model.config_json).map_err(|e| {
        AppCommandError::configuration_invalid("Invalid config JSON").with_detail(e.to_string())
    })?;

    let token = crate::keyring_store::get_channel_token(id).ok_or_else(|| {
        tracing::info!("[connect_chat_channel] channel {id}: Token not set in keyring");
        AppCommandError::configuration_missing("Token not set")
    })?;

    tracing::info!(
        "[connect_chat_channel] channel {id}: creating {channel_type} backend, config={}",
        model.config_json
    );

    let backend = crate::chat_channel::backends::create_backend(id, channel_type, &config, token)
        .map_err(AppCommandError::from)?;

    manager
        .add_channel(id, model.name, channel_type, backend)
        .await
        .map_err(|e| {
            tracing::error!("[connect_chat_channel] channel {id}: add_channel failed: {e}");
            AppCommandError::from(e)
        })?;

    tracing::info!("[connect_chat_channel] channel {id}: connected successfully");
    Ok(())
}

pub async fn test_chat_channel_core(db: &AppDatabase, id: i32) -> Result<(), AppCommandError> {
    let model = chat_channel_service::get_by_id(&db.conn, id)
        .await
        .map_err(AppCommandError::from)?
        .ok_or_else(|| AppCommandError::not_found(format!("Chat channel {id} not found")))?;

    let channel_type: ChannelType = serde_json::from_value(serde_json::Value::String(
        model.channel_type.clone(),
    ))
    .map_err(|_| {
        AppCommandError::configuration_invalid(format!(
            "Invalid channel type: {}",
            model.channel_type
        ))
    })?;

    let config: serde_json::Value = serde_json::from_str(&model.config_json).map_err(|e| {
        AppCommandError::configuration_invalid("Invalid config JSON").with_detail(e.to_string())
    })?;

    let token = crate::keyring_store::get_channel_token(id)
        .ok_or_else(|| AppCommandError::configuration_missing("Token not set"))?;

    let backend = crate::chat_channel::backends::create_backend(id, channel_type, &config, token)
        .map_err(AppCommandError::from)?;

    backend
        .test_connection()
        .await
        .map_err(AppCommandError::from)?;

    Ok(())
}

pub fn save_chat_channel_token_core(channel_id: i32, token: &str) -> Result<(), AppCommandError> {
    crate::keyring_store::set_channel_token(channel_id, token)
        .map_err(|e| AppCommandError::io_error("Failed to save token").with_detail(e))
}

pub fn get_chat_channel_has_token_core(channel_id: i32) -> Result<bool, AppCommandError> {
    Ok(crate::keyring_store::get_channel_token(channel_id).is_some())
}

pub fn delete_chat_channel_token_core(channel_id: i32) -> Result<(), AppCommandError> {
    crate::keyring_store::delete_channel_token(channel_id)
        .map_err(|e| AppCommandError::io_error("Failed to delete token").with_detail(e))
}

pub async fn disconnect_chat_channel_core(
    manager: &ChatChannelManager,
    id: i32,
) -> Result<(), AppCommandError> {
    manager
        .remove_channel(id)
        .await
        .map_err(AppCommandError::from)?;
    Ok(())
}

pub async fn get_chat_channel_status_core(
    manager: &ChatChannelManager,
) -> Result<Vec<ChannelStatusInfo>, AppCommandError> {
    Ok(manager.get_status().await)
}

pub async fn list_chat_channel_messages_core(
    db: &AppDatabase,
    channel_id: i32,
    limit: Option<u64>,
    offset: Option<u64>,
) -> Result<Vec<ChatChannelMessageLogInfo>, AppCommandError> {
    let limit = limit.unwrap_or(50);
    let offset = offset.unwrap_or(0);
    let rows =
        chat_channel_message_log_service::list_by_channel(&db.conn, channel_id, limit, offset)
            .await
            .map_err(AppCommandError::from)?;
    Ok(rows
        .into_iter()
        .map(ChatChannelMessageLogInfo::from)
        .collect())
}

const COMMAND_PREFIX_KEY: &str = "chat_command_prefix";
const DEFAULT_COMMAND_PREFIX: &str = "/";

pub async fn get_chat_command_prefix_core(db: &AppDatabase) -> Result<String, AppCommandError> {
    let val = crate::db::service::app_metadata_service::get_value(&db.conn, COMMAND_PREFIX_KEY)
        .await
        .map_err(AppCommandError::from)?;
    Ok(val.unwrap_or_else(|| DEFAULT_COMMAND_PREFIX.to_string()))
}

pub async fn set_chat_command_prefix_core(
    db: &AppDatabase,
    prefix: String,
) -> Result<(), AppCommandError> {
    let trimmed = prefix.trim();
    if trimmed.is_empty() || trimmed.len() > 3 || trimmed.chars().any(|c| c.is_alphanumeric()) {
        return Err(AppCommandError::invalid_input(
            "Prefix must be 1-3 non-alphanumeric characters",
        ));
    }
    crate::db::service::app_metadata_service::upsert_value(&db.conn, COMMAND_PREFIX_KEY, trimmed)
        .await
        .map_err(AppCommandError::from)?;
    Ok(())
}

const MESSAGE_LANGUAGE_KEY: &str = "chat_message_language";
const DEFAULT_MESSAGE_LANGUAGE: &str = "zh-cn";

pub async fn get_chat_message_language_core(db: &AppDatabase) -> Result<String, AppCommandError> {
    let val = crate::db::service::app_metadata_service::get_value(&db.conn, MESSAGE_LANGUAGE_KEY)
        .await
        .map_err(AppCommandError::from)?;
    Ok(val.unwrap_or_else(|| DEFAULT_MESSAGE_LANGUAGE.to_string()))
}

pub async fn set_chat_message_language_core(
    db: &AppDatabase,
    language: String,
) -> Result<(), AppCommandError> {
    // Validate language code
    let valid = [
        "en", "zh-cn", "zh-tw", "ja", "ko", "es", "de", "fr", "pt", "ar",
    ];
    let lang_lower = language.to_lowercase();
    if !valid.contains(&lang_lower.as_str()) {
        return Err(AppCommandError::invalid_input(format!(
            "Unsupported language: {language}. Supported: {}",
            valid.join(", ")
        )));
    }
    crate::db::service::app_metadata_service::upsert_value(
        &db.conn,
        MESSAGE_LANGUAGE_KEY,
        &lang_lower,
    )
    .await
    .map_err(AppCommandError::from)?;
    crate::chat_channel::event_subscriber::bump_event_config_epoch();
    Ok(())
}

const EVENT_FILTER_KEY: &str = "chat_event_filter";

pub async fn get_chat_event_filter_core(
    db: &AppDatabase,
) -> Result<Option<Vec<String>>, AppCommandError> {
    let val = crate::db::service::app_metadata_service::get_value(&db.conn, EVENT_FILTER_KEY)
        .await
        .map_err(AppCommandError::from)?;
    match val {
        Some(json) => {
            // Parse as Option<Vec<String>> to correctly handle stored "null"
            let filter: Option<Vec<String>> = serde_json::from_str(&json)
                .map_err(|e| AppCommandError::invalid_input(e.to_string()))?;
            Ok(filter)
        }
        None => Ok(None),
    }
}

pub async fn set_chat_event_filter_core(
    db: &AppDatabase,
    filter: Option<Vec<String>>,
) -> Result<(), AppCommandError> {
    match filter {
        Some(arr) => {
            let json = serde_json::to_string(&arr)
                .map_err(|e| AppCommandError::invalid_input(e.to_string()))?;
            crate::db::service::app_metadata_service::upsert_value(
                &db.conn,
                EVENT_FILTER_KEY,
                &json,
            )
            .await
            .map_err(AppCommandError::from)?;
        }
        None => {
            // null is the DEFAULT event set: every event EXCEPT the opt-in ones
            // that export prompt text (see `event_subscriber::DEFAULT_OFF_EVENTS`,
            // e.g. `user_prompt_sent`). Persist the sentinel "null".
            crate::db::service::app_metadata_service::upsert_value(
                &db.conn,
                EVENT_FILTER_KEY,
                "null",
            )
            .await
            .map_err(AppCommandError::from)?;
        }
    }
    crate::chat_channel::event_subscriber::bump_event_config_epoch();
    Ok(())
}

const EVENT_WEBHOOKS_KEY: &str = "chat_event_webhooks";

pub async fn get_chat_event_webhooks_core(
    db: &AppDatabase,
) -> Result<Vec<WebhookConfig>, AppCommandError> {
    let val = crate::db::service::app_metadata_service::get_value(&db.conn, EVENT_WEBHOOKS_KEY)
        .await
        .map_err(AppCommandError::from)?;
    match val {
        Some(json) => {
            let hooks: Vec<WebhookConfig> = serde_json::from_str(&json)
                .map_err(|e| AppCommandError::invalid_input(e.to_string()))?;
            Ok(hooks)
        }
        None => Ok(Vec::new()),
    }
}

pub async fn set_chat_event_webhooks_core(
    db: &AppDatabase,
    webhooks: Vec<WebhookConfig>,
) -> Result<(), AppCommandError> {
    // Trim, drop empty-URL rows, require http(s), dedup by URL (order-preserving,
    // first `enabled` wins). Store the trimmed original (not reqwest's normalized
    // form) so the user's input round-trips unchanged in the UI.
    let mut cleaned: Vec<WebhookConfig> = Vec::new();
    for w in webhooks {
        let url = w.url.trim();
        if url.is_empty() {
            continue;
        }
        let parsed = reqwest::Url::parse(url)
            .map_err(|_| AppCommandError::invalid_input(format!("Invalid webhook URL: {url}")))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(AppCommandError::invalid_input(format!(
                "Webhook URL must use http or https: {url}"
            )));
        }
        if !cleaned.iter().any(|c| c.url == url) {
            cleaned.push(WebhookConfig {
                url: url.to_string(),
                enabled: w.enabled,
            });
        }
    }
    let json = serde_json::to_string(&cleaned)
        .map_err(|e| AppCommandError::invalid_input(e.to_string()))?;
    crate::db::service::app_metadata_service::upsert_value(&db.conn, EVENT_WEBHOOKS_KEY, &json)
        .await
        .map_err(AppCommandError::from)?;
    crate::chat_channel::event_subscriber::bump_event_config_epoch();
    Ok(())
}

pub async fn get_chat_natural_router_config_core(
    db: &AppDatabase,
) -> Result<ChatNaturalRouterConfig, AppCommandError> {
    crate::chat_channel::natural_router_config::get_chat_natural_router_config(&db.conn).await
}

pub async fn set_chat_natural_router_config_core(
    db: &AppDatabase,
    config: ChatNaturalRouterConfigInput,
) -> Result<(), AppCommandError> {
    crate::chat_channel::natural_router_config::set_chat_natural_router_config(&db.conn, config)
        .await
}

pub fn save_chat_natural_router_api_key_core(token: &str) -> Result<(), AppCommandError> {
    crate::chat_channel::natural_router_config::save_chat_natural_router_api_key(token)
}

pub fn delete_chat_natural_router_api_key_core() -> Result<(), AppCommandError> {
    crate::chat_channel::natural_router_config::delete_chat_natural_router_api_key()
}

// ---------------------------------------------------------------------------
// WeChat QR code auth
// ---------------------------------------------------------------------------

pub async fn weixin_get_qrcode_core() -> Result<WeixinQrcodeInfo, AppCommandError> {
    crate::chat_channel::backends::weixin::weixin_get_qrcode()
        .await
        .map_err(AppCommandError::from)
}

pub async fn weixin_check_qrcode_core(
    db: &AppDatabase,
    channel_id: i32,
    qrcode: &str,
) -> Result<WeixinQrcodeStatusPublic, AppCommandError> {
    let result = crate::chat_channel::backends::weixin::weixin_check_qrcode(qrcode)
        .await
        .map_err(AppCommandError::from)?;

    // On confirmed: save token + update config with base_url
    if result.status == "confirmed" {
        tracing::error!(
            "[Weixin] QR confirmed for channel {channel_id}, bot_token={}, base_url={}",
            result
                .bot_token
                .as_deref()
                .map(|t| {
                    // Char-boundary-safe prefix: `&t[..8]` panics if a multibyte
                    // char straddles byte 8.
                    let end = t.char_indices().nth(8).map_or(t.len(), |(i, _)| i);
                    &t[..end]
                })
                .unwrap_or("None"),
            result.base_url.as_deref().unwrap_or("None"),
        );
        if let Some(ref token) = result.bot_token {
            save_chat_channel_token_core(channel_id, token)?;
            tracing::info!("[Weixin] Token saved for channel {channel_id}");
        } else {
            tracing::warn!(
                "[Weixin] WARNING: No bot_token in confirmed response for channel {channel_id}"
            );
        }
        if let Some(ref base_url) = result.base_url {
            let config_json = serde_json::json!({ "base_url": base_url }).to_string();
            update_chat_channel_core(
                db,
                channel_id,
                None,
                None,
                Some(config_json),
                None,
                None,
                None,
            )
            .await?;
            tracing::info!("[Weixin] Config updated with base_url for channel {channel_id}");
        }
    }

    // Return only the status — never expose bot_token to the frontend
    Ok(WeixinQrcodeStatusPublic {
        status: result.status,
    })
}

// ---------------------------------------------------------------------------
// Tauri commands (use tauri::State for injection)
// ---------------------------------------------------------------------------

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn list_chat_channels(
    db: tauri::State<'_, AppDatabase>,
) -> Result<Vec<ChatChannelInfo>, AppCommandError> {
    list_chat_channels_core(&db).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn create_chat_channel(
    db: tauri::State<'_, AppDatabase>,
    name: String,
    channel_type: String,
    config_json: String,
    enabled: bool,
    daily_report_enabled: bool,
    daily_report_time: Option<String>,
) -> Result<ChatChannelInfo, AppCommandError> {
    create_chat_channel_core(
        &db,
        name,
        channel_type,
        config_json,
        enabled,
        daily_report_enabled,
        daily_report_time,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn update_chat_channel(
    db: tauri::State<'_, AppDatabase>,
    id: i32,
    name: Option<String>,
    enabled: Option<bool>,
    config_json: Option<String>,
    event_filter_json: Option<Option<String>>,
    daily_report_enabled: Option<bool>,
    daily_report_time: Option<Option<String>>,
) -> Result<ChatChannelInfo, AppCommandError> {
    update_chat_channel_core(
        &db,
        id,
        name,
        enabled,
        config_json,
        event_filter_json,
        daily_report_enabled,
        daily_report_time,
    )
    .await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn delete_chat_channel(
    db: tauri::State<'_, AppDatabase>,
    manager: tauri::State<'_, ChatChannelManager>,
    id: i32,
) -> Result<(), AppCommandError> {
    delete_chat_channel_core(&db, &manager, id).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn save_chat_channel_token(
    channel_id: i32,
    token: String,
) -> Result<(), AppCommandError> {
    save_chat_channel_token_core(channel_id, &token)
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn get_chat_channel_has_token(channel_id: i32) -> Result<bool, AppCommandError> {
    get_chat_channel_has_token_core(channel_id)
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn delete_chat_channel_token(channel_id: i32) -> Result<(), AppCommandError> {
    delete_chat_channel_token_core(channel_id)
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn connect_chat_channel(
    db: tauri::State<'_, AppDatabase>,
    manager: tauri::State<'_, ChatChannelManager>,
    id: i32,
) -> Result<(), AppCommandError> {
    connect_chat_channel_core(&db, &manager, id).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn disconnect_chat_channel(
    manager: tauri::State<'_, ChatChannelManager>,
    id: i32,
) -> Result<(), AppCommandError> {
    disconnect_chat_channel_core(&manager, id).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn test_chat_channel(
    db: tauri::State<'_, AppDatabase>,
    id: i32,
) -> Result<(), AppCommandError> {
    test_chat_channel_core(&db, id).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn get_chat_channel_status(
    manager: tauri::State<'_, ChatChannelManager>,
) -> Result<Vec<ChannelStatusInfo>, AppCommandError> {
    get_chat_channel_status_core(&manager).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn list_chat_channel_messages(
    db: tauri::State<'_, AppDatabase>,
    channel_id: i32,
    limit: Option<u64>,
    offset: Option<u64>,
) -> Result<Vec<ChatChannelMessageLogInfo>, AppCommandError> {
    list_chat_channel_messages_core(&db, channel_id, limit, offset).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn get_chat_command_prefix(
    db: tauri::State<'_, AppDatabase>,
) -> Result<String, AppCommandError> {
    get_chat_command_prefix_core(&db).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn set_chat_command_prefix(
    db: tauri::State<'_, AppDatabase>,
    prefix: String,
) -> Result<(), AppCommandError> {
    set_chat_command_prefix_core(&db, prefix).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn get_chat_event_filter(
    db: tauri::State<'_, AppDatabase>,
) -> Result<Option<Vec<String>>, AppCommandError> {
    get_chat_event_filter_core(&db).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn set_chat_event_filter(
    db: tauri::State<'_, AppDatabase>,
    filter: Option<Vec<String>>,
) -> Result<(), AppCommandError> {
    set_chat_event_filter_core(&db, filter).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn get_chat_event_webhooks(
    db: tauri::State<'_, AppDatabase>,
) -> Result<Vec<WebhookConfig>, AppCommandError> {
    get_chat_event_webhooks_core(&db).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn set_chat_event_webhooks(
    db: tauri::State<'_, AppDatabase>,
    webhooks: Vec<WebhookConfig>,
) -> Result<(), AppCommandError> {
    set_chat_event_webhooks_core(&db, webhooks).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn get_chat_message_language(
    db: tauri::State<'_, AppDatabase>,
) -> Result<String, AppCommandError> {
    get_chat_message_language_core(&db).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn set_chat_message_language(
    db: tauri::State<'_, AppDatabase>,
    language: String,
) -> Result<(), AppCommandError> {
    set_chat_message_language_core(&db, language).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn get_chat_natural_router_config(
    db: tauri::State<'_, AppDatabase>,
) -> Result<ChatNaturalRouterConfig, AppCommandError> {
    get_chat_natural_router_config_core(&db).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn set_chat_natural_router_config(
    db: tauri::State<'_, AppDatabase>,
    config: ChatNaturalRouterConfigInput,
) -> Result<(), AppCommandError> {
    set_chat_natural_router_config_core(&db, config).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn save_chat_natural_router_api_key(token: String) -> Result<(), AppCommandError> {
    save_chat_natural_router_api_key_core(&token)
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn delete_chat_natural_router_api_key() -> Result<(), AppCommandError> {
    delete_chat_natural_router_api_key_core()
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn weixin_get_qrcode() -> Result<WeixinQrcodeInfo, AppCommandError> {
    weixin_get_qrcode_core().await
}

// ── WeCom (企业微信 / wecom-cli) auth ──

#[derive(Debug, Clone, serde::Serialize)]
pub struct WecomAuthStatus {
    pub cli_installed: bool,
    pub authorized: bool,
    /// Whether the `wecom-cli init` process launched by `wecom_start_auth` is
    /// still alive and able to receive the scan. Lets the UI distinguish "not
    /// scanned yet" from "the pending authorization is gone".
    pub auth_process_running: bool,
    /// Why that process exited, when it exited without authorizing.
    pub auth_process_error: Option<String>,
}

pub async fn wecom_get_auth_status_core() -> Result<WecomAuthStatus, AppCommandError> {
    let cli_installed = crate::chat_channel::backends::wecom::cli_installed();
    let authorized = if cli_installed {
        crate::chat_channel::backends::wecom::auth_status()
            .await
            .unwrap_or(false)
    } else {
        false
    };
    let process = crate::chat_channel::backends::wecom::auth_process_state();
    Ok(WecomAuthStatus {
        cli_installed,
        authorized,
        auth_process_running: process.running,
        auth_process_error: process.last_error,
    })
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct WecomAuthStart {
    pub auth_url: String,
}

/// Install wecom-cli when missing, then launch the QR authorization and hand
/// the link back for the UI to render. Completion is observed by polling
/// `wecom_get_auth_status`.
pub async fn wecom_start_auth_core() -> Result<WecomAuthStart, AppCommandError> {
    let auth_url = crate::chat_channel::backends::wecom::start_auth()
        .await
        .map_err(AppCommandError::from)?;
    Ok(WecomAuthStart { auth_url })
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn wecom_get_auth_status() -> Result<WecomAuthStatus, AppCommandError> {
    wecom_get_auth_status_core().await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn wecom_start_auth() -> Result<WecomAuthStart, AppCommandError> {
    wecom_start_auth_core().await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn weixin_check_qrcode(
    db: tauri::State<'_, AppDatabase>,
    channel_id: i32,
    qrcode: String,
) -> Result<WeixinQrcodeStatusPublic, AppCommandError> {
    weixin_check_qrcode_core(&db, channel_id, &qrcode).await
}
