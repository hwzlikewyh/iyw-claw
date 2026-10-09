// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// How long the driver may take to capture the whole screen.
pub(super) const CAPTURE_TIMEOUT: Duration = Duration::from_secs(60);

/// How long one pointer action on the screen may take, a drag's path aside.
pub(super) const ACT_TIMEOUT: Duration = Duration::from_secs(30);

/// How many pictures are taken before a screen whose never-shared windows
/// keep moving is given up on.
pub(super) const CAPTURE_ATTEMPTS: usize = 3;
