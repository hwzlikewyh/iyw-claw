// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn computer_share_app(
    app: AppHandle,
    target_id: Option<String>,
    app_id: Option<String>,
    level: GrantLevel,
) -> Result<SharedState, AppCommandError> {
    computer_share_app_core(
        &*service(&app)?,
        target_id.as_deref(),
        app_id.as_deref(),
        level,
    )
    .await
}
