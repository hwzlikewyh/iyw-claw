// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl PeerGuard {
    pub(in crate::computer::helper) fn still_peer(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            crate::computer::codesign::peer_audit_token(0).is_ok_and(|now| {
                now.pid() == self.token.pid() && now.pid_version() == self.token.pid_version()
            })
        }
        #[cfg(not(target_os = "macos"))]
        {
            true
        }
    }
}
