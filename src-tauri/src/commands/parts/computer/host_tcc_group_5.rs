// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

#[cfg(not(target_os = "macos"))]
pub(super) fn host_tcc() -> Option<HostTccStatus> {
    None
}
