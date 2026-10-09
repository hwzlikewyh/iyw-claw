// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl WindowIdentity {
    pub(in crate::computer::targets) fn of(window: &RawWindow) -> Self {
        Self {
            pid: window.pid,
            started_at: window.app.started_at,
            window_id: window.window_id,
            content: window.content,
        }
    }
}
