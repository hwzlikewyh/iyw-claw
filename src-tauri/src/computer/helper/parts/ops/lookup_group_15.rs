// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl AppCache {
    #[cfg(not(any(target_os = "macos", windows)))]
    pub(in crate::computer::helper::ops) fn lookup(
        &self,
        pid: u32,
        started_at: Option<u64>,
    ) -> Option<&RawApp> {
        self.apps.get(&(pid, started_at))
    }

    #[cfg(not(any(target_os = "macos", windows)))]
    pub(in crate::computer::helper::ops) fn refill(&mut self, apps: Vec<RawApp>) {
        self.apps = apps
            .into_iter()
            .map(|app| ((app.pid, app.started_at), app))
            .collect();
    }
}
