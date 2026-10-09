// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Move only the group switch, leaving the rest at whatever the database says
/// at the moment of the write. For the status popover.
pub async fn set_computer_tools_enabled_core(
    conn: &DatabaseConnection,
    config: &ComputerToolsRuntimeConfig,
    emitter: &EventEmitter,
    enabled: bool,
) -> Result<ComputerToolsSettings, AppCommandError> {
    let _guard = COMPUTER_TOOLS_WRITE_LOCK.lock().await;
    app_metadata_service::upsert_value(conn, KEY_COMPUTER_TOOLS_ENABLED, &enabled.to_string())
        .await
        .map_err(AppCommandError::from)?;
    let settings = load_computer_tools_settings(conn).await;
    config.set(settings.clone().into_runtime_config()).await;
    emit_event(emitter, COMPUTER_TOOLS_SETTINGS_CHANGED_EVENT, &settings);
    sync_computer_skill(conn, settings.enabled).await;
    Ok(settings)
}

/// The preferences one write moves: each one given is written, each one
/// absent is left at whatever the database says.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerToolsPreferences {
    #[serde(default)]
    pub grant_ttl_minutes: Option<u32>,
    #[serde(default)]
    pub blocklist: Option<Vec<String>>,
    /// Keys of the default entries to leave off the list.
    #[serde(default)]
    pub blocklist_removed: Option<Vec<String>>,
    /// Empty switches the shortcut off.
    #[serde(default)]
    pub stop_shortcut: Option<String>,
    #[serde(default)]
    pub show_indicator: Option<bool>,
    #[serde(default)]
    pub allow_foreground: Option<bool>,
    #[serde(default)]
    pub default_delivery: Option<ActDelivery>,
    #[serde(default)]
    pub launch_enabled: Option<bool>,
    #[serde(default)]
    pub clipboard_enabled: Option<bool>,
    #[serde(default)]
    pub screen_enabled: Option<bool>,
}

/// Move the grant timeout, the user's blocklist (their additions and the
/// defaults they took off), the stop shortcut, the strip, whether a window
/// may be brought to the front and how an action goes by default — only the
/// ones given — leaving everything else at whatever the database says.
/// For the Computer use settings section, which edits these and not the
/// switch (that one lives with the other tool groups, and in the status
/// popover), and which sends only what the person changed: a form that
/// loaded before another window added a blocklist entry must not take it out
/// again by saving a new timeout.
pub async fn set_computer_tools_preferences_core(
    conn: &DatabaseConnection,
    config: &ComputerToolsRuntimeConfig,
    emitter: &EventEmitter,
    preferences: ComputerToolsPreferences,
) -> Result<ComputerToolsSettings, AppCommandError> {
    let ComputerToolsPreferences {
        grant_ttl_minutes,
        blocklist,
        blocklist_removed,
        stop_shortcut,
        show_indicator,
        allow_foreground,
        default_delivery,
        launch_enabled,
        clipboard_enabled,
        screen_enabled,
    } = preferences;
    let blocklist = blocklist
        .map(|list| serde_json::to_string(&normalize_blocklist(list)))
        .transpose()
        .map_err(|e| AppCommandError::configuration_invalid(e.to_string()))?;
    let blocklist_removed = blocklist_removed
        .map(|keys| serde_json::to_string(&normalize_removed(keys)))
        .transpose()
        .map_err(|e| AppCommandError::configuration_invalid(e.to_string()))?;
    let stop_shortcut = stop_shortcut
        .as_deref()
        .map(checked_stop_shortcut)
        .transpose()?;
    let writes: Vec<(&str, String)> = [
        grant_ttl_minutes.map(|m| (KEY_COMPUTER_TOOLS_GRANT_TTL_MINUTES, m.to_string())),
        blocklist.map(|list| (KEY_COMPUTER_TOOLS_BLOCKLIST, list)),
        blocklist_removed.map(|keys| (KEY_COMPUTER_TOOLS_BLOCKLIST_REMOVED, keys)),
        stop_shortcut.map(|s| (KEY_COMPUTER_TOOLS_STOP_SHORTCUT, s)),
        show_indicator.map(|on| (KEY_COMPUTER_TOOLS_SHOW_INDICATOR, on.to_string())),
        allow_foreground.map(|on| (KEY_COMPUTER_TOOLS_ALLOW_FOREGROUND, on.to_string())),
        default_delivery.map(|d| (KEY_COMPUTER_TOOLS_DEFAULT_DELIVERY, d.as_str().to_string())),
        launch_enabled.map(|on| (KEY_COMPUTER_TOOLS_LAUNCH_ENABLED, on.to_string())),
        clipboard_enabled.map(|on| (KEY_COMPUTER_TOOLS_CLIPBOARD_ENABLED, on.to_string())),
        screen_enabled.map(|on| (KEY_COMPUTER_TOOLS_SCREEN_ENABLED, on.to_string())),
    ]
    .into_iter()
    .flatten()
    .collect();
    let _guard = COMPUTER_TOOLS_WRITE_LOCK.lock().await;
    if writes.is_empty() {
        return Ok(load_computer_tools_settings(conn).await);
    }
    for (key, value) in writes {
        app_metadata_service::upsert_value(conn, key, &value)
            .await
            .map_err(AppCommandError::from)?;
    }
    let settings = load_computer_tools_settings(conn).await;
    config.set(settings.clone().into_runtime_config()).await;
    emit_event(emitter, COMPUTER_TOOLS_SETTINGS_CHANGED_EVENT, &settings);
    Ok(settings)
}

/// Persist + apply + broadcast the whole record. Shared by the Tauri command
/// and the HTTP handler.
pub async fn set_computer_tools_settings_core(
    conn: &DatabaseConnection,
    config: &ComputerToolsRuntimeConfig,
    emitter: &EventEmitter,
    desired: ComputerToolsSettings,
) -> Result<ComputerToolsSettings, AppCommandError> {
    let desired = ComputerToolsSettings {
        blocklist: normalize_blocklist(desired.blocklist),
        blocklist_removed: normalize_removed(desired.blocklist_removed),
        blocklist_defaults: default_blocklist(Platform::current()),
        stop_shortcut: checked_stop_shortcut(&desired.stop_shortcut)?,
        ..desired
    };
    let _guard = COMPUTER_TOOLS_WRITE_LOCK.lock().await;
    let blocklist = serde_json::to_string(&desired.blocklist)
        .map_err(|e| AppCommandError::configuration_invalid(e.to_string()))?;
    let blocklist_removed = serde_json::to_string(&desired.blocklist_removed)
        .map_err(|e| AppCommandError::configuration_invalid(e.to_string()))?;
    for (key, value) in [
        (KEY_COMPUTER_TOOLS_ENABLED, desired.enabled.to_string()),
        (
            KEY_COMPUTER_TOOLS_GRANT_TTL_MINUTES,
            desired.grant_ttl_minutes.to_string(),
        ),
        (KEY_COMPUTER_TOOLS_BLOCKLIST, blocklist),
        (KEY_COMPUTER_TOOLS_BLOCKLIST_REMOVED, blocklist_removed),
        (
            KEY_COMPUTER_TOOLS_STOP_SHORTCUT,
            desired.stop_shortcut.clone(),
        ),
        (
            KEY_COMPUTER_TOOLS_SHOW_INDICATOR,
            desired.show_indicator.to_string(),
        ),
        (
            KEY_COMPUTER_TOOLS_ALLOW_FOREGROUND,
            desired.allow_foreground.to_string(),
        ),
        (
            KEY_COMPUTER_TOOLS_DEFAULT_DELIVERY,
            desired.default_delivery.as_str().to_string(),
        ),
        (
            KEY_COMPUTER_TOOLS_LAUNCH_ENABLED,
            desired.launch_enabled.to_string(),
        ),
        (
            KEY_COMPUTER_TOOLS_CLIPBOARD_ENABLED,
            desired.clipboard_enabled.to_string(),
        ),
        (
            KEY_COMPUTER_TOOLS_SCREEN_ENABLED,
            desired.screen_enabled.to_string(),
        ),
    ] {
        app_metadata_service::upsert_value(conn, key, &value)
            .await
            .map_err(AppCommandError::from)?;
    }
    config.set(desired.clone().into_runtime_config()).await;
    emit_event(emitter, COMPUTER_TOOLS_SETTINGS_CHANGED_EVENT, &desired);
    sync_computer_skill(conn, desired.enabled).await;
    Ok(desired)
}

// -------- Tauri commands -----------------------------------------------------

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn get_computer_tools_settings(
    #[cfg(feature = "tauri-runtime")] db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<ComputerToolsSettings, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        Ok(load_computer_tools_settings(&db.conn).await)
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn set_computer_tools_settings(
    #[cfg(feature = "tauri-runtime")] app: tauri::AppHandle,
    #[cfg(feature = "tauri-runtime")] db: tauri::State<'_, crate::db::AppDatabase>,
    #[cfg(feature = "tauri-runtime")] config: tauri::State<'_, ComputerToolsRuntimeConfig>,
    settings: ComputerToolsSettings,
) -> Result<ComputerToolsSettings, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        let emitter = EventEmitter::Tauri(app);
        set_computer_tools_settings_core(&db.conn, &config, &emitter, settings).await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        let _ = settings;
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn set_computer_tools_enabled(
    #[cfg(feature = "tauri-runtime")] app: tauri::AppHandle,
    #[cfg(feature = "tauri-runtime")] db: tauri::State<'_, crate::db::AppDatabase>,
    #[cfg(feature = "tauri-runtime")] config: tauri::State<'_, ComputerToolsRuntimeConfig>,
    enabled: bool,
) -> Result<ComputerToolsSettings, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        let emitter = EventEmitter::Tauri(app);
        set_computer_tools_enabled_core(&db.conn, &config, &emitter, enabled).await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        let _ = enabled;
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}
