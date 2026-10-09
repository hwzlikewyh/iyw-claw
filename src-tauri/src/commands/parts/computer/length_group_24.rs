// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl Presses {
    /// How long a held key is held; nothing for a count of presses.
    pub(in crate::commands::computer) fn length(self) -> Option<Duration> {
        match self {
            Presses::Held(length) => Some(length),
            Presses::Count(_) => None,
        }
    }

    pub(in crate::commands::computer) fn of(request: &ComputerActRequest) -> Self {
        match request {
            ComputerActRequest::Key { repeat, .. } => {
                Presses::Count((*repeat).clamp(1, MAX_KEY_REPEAT))
            }
            ComputerActRequest::HoldKey { duration_ms, .. } => Presses::Held(
                Duration::from_millis(u64::from((*duration_ms).min(MAX_HOLD_MS))),
            ),
            _ => Presses::Count(1),
        }
    }

    /// What the report says of the presses: how many went out, for a key
    /// pressed more than once or held.
    pub(in crate::commands::computer) fn reported(self, pressed: u32) -> Option<u32> {
        match self {
            Presses::Count(count) => (count > 1).then_some(count),
            Presses::Held(_) => Some(pressed),
        }
    }

    /// How a refusal after `pressed` presses begins: what did go out, and
    /// whether the press that failed may have gone out too.
    pub(in crate::commands::computer) fn cut_short(self, pressed: u32, maybe_done: bool) -> String {
        let after = if maybe_done {
            "; the press after that may or may not have gone out: "
        } else {
            ", then stopped: "
        };
        match self {
            Presses::Count(count) => {
                format!("The key was pressed {pressed} of {count} times{after}")
            }
            Presses::Held(_) => format!("The key was held for {pressed} presses{after}"),
        }
    }
}
