// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The answer where there is no desktop: server mode, and the stub in every
/// test that does not care about one.
pub struct NoComputerDesktop;
