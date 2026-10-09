// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

pub(super) type AXError = i32;

pub(super) const AX_SUCCESS: AXError = 0;

pub(super) const AX_FAILURE: AXError = -25200;

pub(super) const AX_NO_VALUE: AXError = -25212;

/// How long one question to an application may take before it counts as no
/// answer. A busy application answers well within it; a hung one would
/// otherwise hold each question for the system's six seconds.
pub(super) const TIMEOUT: f32 = 0.5;
