// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn computer_window_thumbnail(
    app: AppHandle,
    target_id: String,
) -> Result<Option<String>, AppCommandError> {
    computer_window_thumbnail_core(&*service(&app)?, &target_id).await
}
