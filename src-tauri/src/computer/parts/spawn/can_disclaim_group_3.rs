// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Whether this system can launch a process as its own TCC principal.
pub fn can_disclaim() -> bool {
    disclaim_fn().is_some()
}
