// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The helper as checked: the process on the other end of the socket when
/// its first frame arrived.
pub(super) struct VerifiedPeer {
    #[cfg(target_os = "macos")]
    pub(in crate::computer::local) fd: i32,
    #[cfg(target_os = "macos")]
    pub(in crate::computer::local) token: crate::computer::codesign::AuditToken,
}
