// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn computer_share_windows(
    app: AppHandle,
    target_ids: Vec<String>,
    level: GrantLevel,
) -> Result<ShareManyResult, AppCommandError> {
    computer_share_windows_core(&*service(&app)?, &target_ids, level).await
}
