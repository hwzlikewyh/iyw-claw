// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

#[cfg(not(windows))]
pub(super) const EDGE: f64 = 6.0;
