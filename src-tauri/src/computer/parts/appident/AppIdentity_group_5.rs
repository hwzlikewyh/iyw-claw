// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// An application, as the process running it shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppIdentity {
    /// The application bundle.
    pub path: String,
    /// `CFBundleIdentifier` from the bundle's `Info.plist`.
    pub bundle_id: String,
    /// What the bundle's `Info.plist` calls the application, untranslated
    /// (`CFBundleDisplayName`, `CFBundleName`).
    pub plist_names: Vec<String>,
    /// The process runs a helper inside the application, not the application
    /// itself.
    pub nested: bool,
}
