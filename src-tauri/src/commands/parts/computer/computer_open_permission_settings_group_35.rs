// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Open System Settings at the pane for one permission.
#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn computer_open_permission_settings(
    app: AppHandle,
    permission: OsPermission,
) -> Result<(), AppCommandError> {
    let Some(url) = permission_settings_url(permission) else {
        return Ok(());
    };
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| AppCommandError::configuration_invalid(e.to_string()))
}

/// Show iyw-computer-helper in the Finder — for dragging it into System
/// Settings' list by hand, should it not be listed there after a request.
#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn computer_reveal_helper(app: AppHandle) -> Result<(), AppCommandError> {
    let helper = crate::computer::local::helper_to_reveal()
        .await
        .map_err(backend_error)?;
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .reveal_item_in_dir(helper)
        .map_err(|e| AppCommandError::configuration_invalid(e.to_string()))
}
