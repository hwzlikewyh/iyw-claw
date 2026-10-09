// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn computer_driver_info(app: AppHandle) -> Result<DriverInfo, AppCommandError> {
    Ok(computer_driver_info_core(&*service(&app)?))
}
