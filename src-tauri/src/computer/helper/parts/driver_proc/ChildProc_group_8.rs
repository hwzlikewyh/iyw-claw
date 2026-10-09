// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

pub(super) enum ChildProc {
    #[cfg(target_os = "macos")]
    Mac(crate::computer::spawn::Child),
    #[cfg(not(target_os = "macos"))]
    Tokio {
        child: tokio::sync::Mutex<tokio::process::Child>,
        exited: std::sync::atomic::AtomicBool,
    },
}
