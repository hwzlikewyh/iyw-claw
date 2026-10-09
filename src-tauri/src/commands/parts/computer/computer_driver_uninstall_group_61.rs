// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn computer_driver_uninstall(
    app: AppHandle,
    db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<DriverInfo, AppCommandError> {
    computer_driver_uninstall_core(&*service(&app)?, &db.conn).await
}

/// The strip's page, telling how large it drew itself (logical pixels).
#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn computer_indicator_fit(app: AppHandle, width: f64, height: f64) {
    crate::computer::indicator::fit(&app, width, height);
}
