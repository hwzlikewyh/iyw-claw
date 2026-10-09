// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Everything but typing: one click, one key, one value.
pub(super) const ACT_TIMEOUT: Duration = Duration::from_secs(30);

/// Typing: the driver budgets up to 100 s of synthesized keystrokes for one
/// call and refuses more before sending any.
pub(super) const TYPE_TIMEOUT: Duration = Duration::from_secs(130);

/// Measuring a window before a point is clicked in it.
pub(super) const MEASURE_TIMEOUT: Duration = Duration::from_secs(15);

/// How long a restored window has to be seen on the screen again: the Dock's
/// or the taskbar's animation, and an application slow to draw.
pub(super) const RESTORE_WAIT: Duration = Duration::from_secs(3);

pub(super) const RESTORE_POLL: Duration = Duration::from_millis(100);

/// The driver's intermediate pointer moves along a drag: one every 25 ms of
/// its path, within the driver's own bounds.
pub(super) const DRAG_STEP_MS: u32 = 25;

pub(super) const MAX_DRAG_STEPS: u32 = 200;

/// How many windows' latest snapshots the helper remembers. The driver keeps
/// eight per process; this bounds the helper's memory, not the driver's.
pub(super) const BOOK_WINDOWS: usize = 64;
