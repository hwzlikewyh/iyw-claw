// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

pub(super) const LIST_TIMEOUT: Duration = Duration::from_secs(30);

/// A window-state read: an accessibility walk of at most [`WALK_BUDGET_MS`],
/// the capture, and the encode.
pub(super) const WINDOW_STATE_TIMEOUT: Duration = Duration::from_secs(60);

/// How long the driver may walk a window's accessibility tree for a
/// snapshot, in milliseconds. Its own default is a second, which cuts a
/// large application's tree short; this is the twenty seconds macOS had
/// before it, and within [`WINDOW_STATE_TIMEOUT`] even where the driver
/// waits twice that and five seconds more for an application to answer.
pub(super) const WALK_BUDGET_MS: u64 = 20_000;

/// Added to the caller's own `timeoutMs` for `verify_state`.
pub(super) const VERIFY_OVERHEAD: Duration = Duration::from_secs(30);

/// The driver's own bounds on a verify.
pub(super) const MAX_VERIFY_TIMEOUT_MS: u32 = 10_000;

pub(super) const MAX_STABLE_SAMPLES: u32 = 5;
