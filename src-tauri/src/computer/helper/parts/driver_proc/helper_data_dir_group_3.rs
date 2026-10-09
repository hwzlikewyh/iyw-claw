// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Where the helper keeps the driver's per-launch home directories.
///
/// Computed from the account database, not from `$HOME`: the helper's
/// decisions do not read the environment it inherited.
pub fn helper_data_dir() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        let home = account_home_dir()?;
        let base = if cfg!(target_os = "macos") {
            home.join("Library").join("Application Support")
        } else {
            home.join(".local").join("share")
        };
        Some(base.join("app.iywclaw").join("computer-helper"))
    }
    #[cfg(windows)]
    {
        dirs::data_local_dir().map(|d| d.join("app.iywclaw").join("computer-helper"))
    }
}
