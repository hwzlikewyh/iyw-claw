// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl ComputerGrant {
    pub fn new(level: GrantLevel, now: i64) -> Self {
        Self {
            level,
            granted_at: now,
            last_used_at: now,
            scope: GrantScope::Window,
        }
    }

    /// A window's share of its application's grant.
    pub fn of_app(level: GrantLevel, now: i64) -> Self {
        Self {
            scope: GrantScope::App,
            ..Self::new(level, now)
        }
    }

    /// A window's share of the entire screen's grant.
    pub fn of_screen(level: GrantLevel, now: i64) -> Self {
        Self {
            scope: GrantScope::Screen,
            ..Self::new(level, now)
        }
    }

    /// Whether the grant has gone unused for longer than `ttl`.
    ///
    /// Unlike a browser grant, which ends only when the person takes it back
    /// or the page leaves its origin, a window grant also lapses on its own.
    /// A tab shows one site; a shared window is often the user's whole working
    /// context in some application — a mail client, a terminal — and "shared
    /// this morning for one question" should not quietly mean "readable all
    /// week". The clock runs from the last read, not from the grant, so a
    /// window an agent is actively using stays shared. `None` is the person's
    /// own choice of "until I take it back".
    ///
    /// Measured on the wall clock, which is what keeps counting while the
    /// machine sleeps (a monotonic clock here stops, and a grant would outlive
    /// a night asleep). A clock that has gone back by more than
    /// [`MAX_CLOCK_SKEW_MS`] since the last read ends the grant: how long it
    /// has been idle can no longer be told, and the answer that is safe is
    /// "too long".
    pub fn lapsed(&self, now: i64, ttl: Option<Duration>) -> bool {
        let Some(ttl) = ttl else {
            return false;
        };
        let idle_since = self.granted_at.max(self.last_used_at);
        if idle_since.saturating_sub(now) > MAX_CLOCK_SKEW_MS {
            return true;
        }
        let ttl_ms = i64::try_from(ttl.as_millis()).unwrap_or(i64::MAX);
        now.saturating_sub(idle_since) >= ttl_ms
    }
}
