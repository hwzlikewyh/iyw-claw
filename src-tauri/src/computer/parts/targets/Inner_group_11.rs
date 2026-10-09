// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

#[derive(Default)]
pub(super) struct Inner {
    pub(in crate::computer::targets) next_id: u64,
    pub(in crate::computer::targets) entries: HashMap<String, TargetEntry>,
    pub(in crate::computer::targets) by_identity: HashMap<WindowIdentity, String>,
    pub(in crate::computer::targets) next_app_id: u64,
    pub(in crate::computer::targets) apps: HashMap<AppIdentity, AppShare>,
    pub(in crate::computer::targets) screen: Option<ScreenShare>,
    /// Sharings of the screen so far: each one's epoch.
    pub(in crate::computer::targets) screen_epochs: u64,
}
