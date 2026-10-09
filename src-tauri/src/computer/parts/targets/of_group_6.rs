// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl AppIdentity {
    /// The application `entry` is a window of; `None` for one that cannot be
    /// told (see [`NotGrantable::Unidentified`]).
    pub fn of(entry: &TargetEntry) -> Option<Self> {
        Some(Self {
            pid: entry.identity.pid,
            started_at: entry.identity.started_at?,
            key: entry.app.key()?.to_string(),
            content: entry.identity.content,
        })
    }

    /// Whether `app`, as a listing of applications names it, is this one.
    pub(in crate::computer::targets) fn names(&self, app: &RawApp) -> bool {
        self.content.is_none()
            && app.pid == self.pid
            && app.started_at == Some(self.started_at)
            && app.key() == Some(self.key.as_str())
    }
}
