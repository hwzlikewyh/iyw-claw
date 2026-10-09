// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn computer_stop_key_status(app: AppHandle) -> Result<StopKeyStatus, AppCommandError> {
    Ok(computer_stop_key_status_core(&*service(&app)?))
}
