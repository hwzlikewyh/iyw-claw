use std::path::PathBuf;

pub(super) fn directory() -> PathBuf {
    crate::paths::iyw_claw_user_dir().join("browser/profiles/webview")
}

#[cfg(target_os = "macos")]
pub(super) fn configure(
    builder: tauri_runtime_wry::wry::WebViewBuilder<'_>,
) -> Result<tauri_runtime_wry::wry::WebViewBuilder<'_>, String> {
    use objc2_foundation::{NSProcessInfo, NSUUID};
    use objc2_web_kit::{WKWebViewConfiguration, WKWebsiteDataStore};
    use tauri_runtime_wry::wry::WebViewBuilderExtMacos;
    let mtm = objc2::MainThreadMarker::new().ok_or("Browser views need the main thread")?;
    let config = unsafe { WKWebViewConfiguration::new(mtm) };
    let version = NSProcessInfo::processInfo().operatingSystemVersion();
    if version.majorVersion >= 14 {
        let identifier = NSUUID::from_bytes([
            0x6e, 0xf6, 0x3a, 0x2e, 0xd0, 0xc6, 0x4a, 0x59, 0xb5, 0x32, 0xdb, 0xd6, 0x3b, 0x85,
            0xa1, 0xe6,
        ]);
        let store = unsafe { WKWebsiteDataStore::dataStoreForIdentifier(&identifier, mtm) };
        unsafe {
            config.setWebsiteDataStore(&store);
        }
    }
    // 旧 macOS 保留持久默认数据存储，不能使用临时 data store 丢掉登录。
    Ok(builder.with_webview_configuration(config))
}
