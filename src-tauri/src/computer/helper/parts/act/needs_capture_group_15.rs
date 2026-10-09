// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Whether `error` is the driver holding no capture of the window to aim a
/// point by: nothing was sent, and a snapshot gives it one.
pub fn needs_capture(error: &HelperError) -> bool {
    error.code == HelperErrorCode::StaleRef && error.message == NO_CAPTURE
}
