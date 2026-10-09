// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn computer_share_window(
    app: AppHandle,
    target_id: String,
    level: GrantLevel,
) -> Result<Vec<SharedWindow>, AppCommandError> {
    computer_share_window_core(&*service(&app)?, &target_id, level).await
}
