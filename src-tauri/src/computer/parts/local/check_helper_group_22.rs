// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

#[cfg(not(target_os = "macos"))]
pub(super) fn check_helper(
    _peer_fd: PeerFd,
    _peer: PeerCheck,
) -> Result<Option<VerifiedPeer>, String> {
    Ok(None)
}
