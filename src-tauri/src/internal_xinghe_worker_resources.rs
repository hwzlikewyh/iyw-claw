use std::path::PathBuf;

/// 早期 worker 分派尚无 AppHandle，复用 Tauri 的平台资源目录算法。
pub(super) fn resource_root() -> Option<PathBuf> {
    let config: serde_json::Value =
        serde_json::from_str(include_str!("../tauri.conf.json")).ok()?;
    let package = tauri::PackageInfo {
        name: config["productName"]
            .as_str()
            .unwrap_or(env!("CARGO_PKG_NAME"))
            .to_string(),
        version: env!("CARGO_PKG_VERSION").parse().ok()?,
        authors: env!("CARGO_PKG_AUTHORS"),
        description: env!("CARGO_PKG_DESCRIPTION"),
        crate_name: env!("CARGO_PKG_NAME"),
    };
    tauri::utils::platform::resource_dir(&package, &Default::default()).ok()
}
