// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

#[cfg(target_os = "macos")]
pub(super) fn host_tcc() -> Option<HostTccStatus> {
    use crate::computer::tcc;
    let me = std::process::id();
    Some(HostTccStatus {
        accessibility: tcc::accessibility_granted(),
        screen_recording: tcc::screen_recording_granted(),
        self_responsible: tcc::responsible_pid(me) == Some(me),
    })
}
