//! 同一签名 PC 包读取安装器保存的品牌配置。
use std::sync::OnceLock;

#[path = "brand_runtime.rs"]
mod runtime;
#[cfg(feature = "tauri-runtime")]
#[path = "brand_desktop.rs"]
mod desktop;
#[cfg(feature = "tauri-runtime")]
pub use desktop::{configure, plugin};

pub const PRODUCT: &str = "iyw-claw";
pub const PACKAGE_IDENTIFIER: &str = "app.iywclaw";
static BRAND: OnceLock<Result<runtime::Brand, String>> = OnceLock::new();
static FALLBACK: OnceLock<runtime::Brand> = OnceLock::new();

pub fn initialize() -> Result<(), String> {
    BRAND.get_or_init(runtime::load).as_ref().map(|_| ()).map_err(Clone::clone)
}

fn current() -> &'static runtime::Brand {
    BRAND.get_or_init(runtime::load).as_ref().unwrap_or_else(|_| FALLBACK.get_or_init(Default::default))
}

pub fn id() -> &'static str { &current().id }
pub fn name() -> &'static str { &current().name }
pub fn identifier() -> &'static str { &current().identifier }
pub fn scheme() -> &'static str { &current().scheme }
pub fn credential_service() -> &'static str { &current().credential_service }
pub fn release_id() -> &'static str { &current().release_id }
pub fn promotion_channel() -> &'static str { &current().promotion_channel }
pub fn describe() -> serde_json::Value {
    serde_json::json!({
        "capability": "IYW_INSTALLED_BRAND_V1", "productKey": PRODUCT,
        "brandId": id(), "displayName": name(), "identifier": identifier(),
        "urlScheme": scheme(), "userDirectory": user_directory_name(), "hasLogo": current().logo.is_some(),
    })
}
pub fn default_web_port() -> u16 { if id() == "iyw" { 3080 } else { 0 } }

pub fn user_directory_name() -> String {
    if id() == "iyw" { ".iyw-claw".into() } else { format!(".iyw-claw-{}", id()) }
}

pub fn prepare_environment() {
    std::env::set_var("IYW_CLAW_RELEASE_CHANNEL", if release_id().is_empty() { "stable" } else { "beta" });
    if id() == "iyw" {
        clear_inherited_brand();
        return;
    }
    std::env::remove_var("IYW_CLAW_INSTALL_ROOT");
    let Some(home) = dirs::home_dir() else { return; };
    let root = home.join(user_directory_name());
    for (key, path) in [
        ("IYW_CLAW_HOME", root.clone()), ("IYW_CLAW_DATA_DIR", root.join("data")),
        ("IYW_CLAW_USER_MEMORY_DIR", root.join("memory")), ("IYW_CLAW_LOG_DIR", root.join("logs")),
        ("IYW_CLAW_BRAND_HOME", root),
    ] { std::env::set_var(key, path); }
    std::env::set_var("IYW_CLAW_BRAND_ID", id());
    std::env::set_var("IYW_CLAW_BRAND_IDENTIFIER", identifier());
}

fn clear_inherited_brand() {
    if !std::env::var("IYW_CLAW_BRAND_ID").is_ok_and(|value| value != id()) { return; }
    for key in ["IYW_CLAW_HOME", "IYW_CLAW_DATA_DIR", "IYW_CLAW_USER_MEMORY_DIR",
        "IYW_CLAW_LOG_DIR", "IYW_CLAW_BRAND_HOME", "IYW_CLAW_INSTALL_ROOT", "IYW_CLAW_BRAND_IDENTIFIER"] {
        std::env::remove_var(key);
    }
    std::env::set_var("IYW_CLAW_BRAND_ID", id());
}
