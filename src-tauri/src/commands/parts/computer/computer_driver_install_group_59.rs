// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn computer_driver_install(app: AppHandle) -> Result<DriverInfo, AppCommandError> {
    computer_driver_install_core(&*service(&app)?).await
}
