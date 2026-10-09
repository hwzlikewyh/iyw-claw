// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

// One argument per preference, as the web handler takes them: the page sends
// the same flat object to both.
#[allow(clippy::too_many_arguments)]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn set_computer_tools_preferences(
    #[cfg(feature = "tauri-runtime")] app: tauri::AppHandle,
    #[cfg(feature = "tauri-runtime")] db: tauri::State<'_, crate::db::AppDatabase>,
    #[cfg(feature = "tauri-runtime")] config: tauri::State<'_, ComputerToolsRuntimeConfig>,
    grant_ttl_minutes: Option<u32>,
    blocklist: Option<Vec<String>>,
    blocklist_removed: Option<Vec<String>>,
    stop_shortcut: Option<String>,
    show_indicator: Option<bool>,
    allow_foreground: Option<bool>,
    default_delivery: Option<ActDelivery>,
    launch_enabled: Option<bool>,
    clipboard_enabled: Option<bool>,
    screen_enabled: Option<bool>,
) -> Result<ComputerToolsSettings, AppCommandError> {
    let preferences = ComputerToolsPreferences {
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
    };
    #[cfg(feature = "tauri-runtime")]
    {
        let emitter = EventEmitter::Tauri(app);
        set_computer_tools_preferences_core(&db.conn, &config, &emitter, preferences).await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        let _ = preferences;
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}
