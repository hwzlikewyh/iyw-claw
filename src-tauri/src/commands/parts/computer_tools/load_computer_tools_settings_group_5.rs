// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Read the persisted keys, falling back to the defaults for a missing or
/// malformed value. Never errors hard.
pub async fn load_computer_tools_settings(conn: &DatabaseConnection) -> ComputerToolsSettings {
    let mut settings = ComputerToolsSettings::default();
    let get = |key: &'static str| async move {
        app_metadata_service::get_value(conn, key)
            .await
            .ok()
            .flatten()
    };
    if let Some(v) = get(KEY_COMPUTER_TOOLS_ENABLED)
        .await
        .and_then(|r| r.parse().ok())
    {
        settings.enabled = v;
    }
    if let Some(v) = get(KEY_COMPUTER_TOOLS_GRANT_TTL_MINUTES)
        .await
        .and_then(|r| r.parse().ok())
    {
        settings.grant_ttl_minutes = v;
    }
    if let Some(v) = get(KEY_COMPUTER_TOOLS_BLOCKLIST)
        .await
        .and_then(|r| serde_json::from_str::<Vec<String>>(&r).ok())
    {
        settings.blocklist = normalize_blocklist(v);
    }
    if let Some(v) = get(KEY_COMPUTER_TOOLS_BLOCKLIST_REMOVED)
        .await
        .and_then(|r| serde_json::from_str::<Vec<String>>(&r).ok())
    {
        settings.blocklist_removed = normalize_removed(v);
    }
    if let Some(v) = get(KEY_COMPUTER_TOOLS_STOP_SHORTCUT).await {
        settings.stop_shortcut = stored_stop_shortcut(&v);
    }
    if let Some(v) = get(KEY_COMPUTER_TOOLS_SHOW_INDICATOR)
        .await
        .and_then(|r| r.parse().ok())
    {
        settings.show_indicator = v;
    }
    if let Some(v) = get(KEY_COMPUTER_TOOLS_ALLOW_FOREGROUND)
        .await
        .and_then(|r| r.parse().ok())
    {
        settings.allow_foreground = v;
    }
    if let Some(v) = get(KEY_COMPUTER_TOOLS_DEFAULT_DELIVERY)
        .await
        .and_then(|r| stored_delivery(&r))
    {
        settings.default_delivery = v;
    }
    if let Some(v) = get(KEY_COMPUTER_TOOLS_LAUNCH_ENABLED)
        .await
        .and_then(|r| r.parse().ok())
    {
        settings.launch_enabled = v;
    }
    if let Some(v) = get(KEY_COMPUTER_TOOLS_CLIPBOARD_ENABLED)
        .await
        .and_then(|r| r.parse().ok())
    {
        settings.clipboard_enabled = v;
    }
    if let Some(v) = get(KEY_COMPUTER_TOOLS_SCREEN_ENABLED)
        .await
        .and_then(|r| r.parse().ok())
    {
        settings.screen_enabled = v;
    }
    settings
}

/// Pull settings from the DB onto the shared runtime handle. Idempotent — safe
/// on startup or after any save.
pub async fn apply_persisted_computer_tools_config(
    conn: &DatabaseConnection,
    config: &ComputerToolsRuntimeConfig,
) {
    let settings = load_computer_tools_settings(conn).await;
    sync_computer_skill(conn, settings.enabled).await;
    config.set(settings.into_runtime_config()).await;
}
