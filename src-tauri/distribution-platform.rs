// Win7 构建在环境、代理和更新请求中使用独立目标，禁止回退到普通 Windows 包。
pub const TARGET: &str = if cfg!(all(windows, target_vendor = "win7")) {
    "windows7"
} else if cfg!(target_os = "macos") {
    "darwin"
} else {
    std::env::consts::OS
};
