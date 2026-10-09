// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn computer_list_shareable_windows(
    app: AppHandle,
) -> Result<Vec<PickerWindow>, AppCommandError> {
    computer_list_shareable_windows_core(&*service(&app)?).await
}
