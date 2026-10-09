// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The iyw-claw that was checked, for checking every request against.
///
/// The kernel's peer token names the last process to have used the other end
/// of the socket, not the one that created it — so a single check at start
/// would vouch for whoever wrote next. Each request is held to the process
/// that passed the check: same pid, same incarnation of it.
pub struct PeerGuard {
    #[cfg(target_os = "macos")]
    pub(in crate::computer::helper) token: crate::computer::codesign::AuditToken,
}
