#[tauri::command]
pub async fn internet_tools_open_extension_settings(browser: String) -> Result<(), String> {
    drop(browser);
    Err("OpenCLI 扩展已停用，请使用内置浏览器".into())
}

#[tauri::command]
pub async fn internet_tools_prepare_extension() -> Result<String, String> {
    Err("OpenCLI 扩展已停用，已有扩展文件保持不变".into())
}
