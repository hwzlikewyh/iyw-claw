// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// One press that went out.
pub(super) struct Press {
    pub(in crate::commands::computer) raw: RawAct,
    pub(in crate::commands::computer) aim: Aim,
    pub(in crate::commands::computer) delivery: ActDelivery,
    /// When it was sent: a held key's time runs from its first press.
    pub(in crate::commands::computer) sent_at: tokio::time::Instant,
    /// The Stop count it was held to.
    pub(in crate::commands::computer) stop: u64,
    /// The sharing it went out under (see `TargetEntry::epoch`).
    pub(in crate::commands::computer) epoch: u64,
}

/// What a press after the first of one key is held to: the Stop count and
/// the sharing the first went out under, and for a held key when its time
/// is up.
#[derive(Debug, Clone, Copy)]
pub(super) struct LaterPress {
    pub(in crate::commands::computer) stop: u64,
    pub(in crate::commands::computer) epoch: u64,
    pub(in crate::commands::computer) until: Option<tokio::time::Instant>,
}

/// A held key goes in once, then again after the delay a held key waits
/// before it repeats, then at the rate it repeats — as fast as the
/// presses go through, and no faster.
pub(super) const HOLD_DELAY: Duration = Duration::from_millis(500);

pub(super) const HOLD_INTERVAL: Duration = Duration::from_millis(50);

/// How many times an action goes out: once; `repeat` times back to back for
/// a key; for a held key, as often as its time allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Presses {
    Count(u32),
    Held(Duration),
}
