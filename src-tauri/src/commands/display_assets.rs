#[cfg(feature = "tauri-runtime")]
use crate::app_error::AppCommandError;

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn read_display_asset(hash: String) -> Result<tauri::ipc::Response, AppCommandError> {
    let asset = crate::display_assets::read(hash.trim()).await?;
    Ok(tauri::ipc::Response::new(asset.bytes))
}
