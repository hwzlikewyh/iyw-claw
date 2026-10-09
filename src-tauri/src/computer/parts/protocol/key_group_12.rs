// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl RawApp {
    /// The stable name of the application: its bundle identifier where it has
    /// one, its path otherwise. `None` for a process the platform describes
    /// by neither, which can be listed but never matched by a blocklist.
    pub fn key(&self) -> Option<&str> {
        self.bundle_id
            .as_deref()
            .filter(|s| !s.is_empty())
            .or(self.path.as_deref().filter(|s| !s.is_empty()))
    }
}
