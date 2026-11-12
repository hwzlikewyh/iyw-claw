fn main() {
    println!("cargo:rerun-if-env-changed=IYW_SHARE_PUBLISHER");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let publisher = std::env::var("IYW_SHARE_PUBLISHER")
        .unwrap_or_else(|_| "CN=IywAssistant Development".into());
    let escaped = publisher
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let template = include_str!("../native-share/windows/application.manifest");
    let path = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap())
        .join("share-target.manifest");
    std::fs::write(&path, template.replace("{{publisher}}", &escaped)).unwrap();
    println!("cargo:rerun-if-changed=../native-share/windows/application.manifest");
    #[cfg(windows)]
    tauri_winres::WindowsResource::new()
        .set_icon("../icons/icon.ico")
        .set_manifest(&std::fs::read_to_string(path).unwrap())
        .set("ProductName", "原助理")
        .set("FileDescription", "原助理系统分享接收程序")
        .compile()
        .expect("Cannot embed the native share target identity and icon");
}
