// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Check the process that sent the first frame is our helper. `None` for a
/// development build, which checks nothing.
#[cfg(target_os = "macos")]
pub(super) fn check_helper(
    peer_fd: PeerFd,
    peer: PeerCheck,
) -> Result<Option<VerifiedPeer>, String> {
    use crate::computer::codesign::{check_guest, peer_audit_token, Guest};
    let Some(requirement) = HELPER_REQUIREMENT.filter(|r| !r.trim().is_empty()) else {
        tracing::warn!("[computer] development build: not checking the helper's signature");
        return Ok(None);
    };
    // A release iyw-claw only ever launches a release helper, which checks
    // iyw-claw in turn; one that did not is not the helper that shipped.
    if peer != PeerCheck::Verified {
        return Err(
            "the helper did not check iyw-claw's signature; it is not the release helper".into(),
        );
    }
    let fd = peer_fd.ok_or("no socket to check")?;
    let token = peer_audit_token(fd).map_err(|e| format!("no peer token for the helper: {e}"))?;
    let info = check_guest(Guest::Audit(token), requirement)
        .map_err(|e| format!("the helper is not iyw-claw's: {e}"))?;
    info.entitlements_clean()
        .map_err(|e| format!("the helper is not one iyw-claw trusts: {e}"))?;
    Ok(Some(VerifiedPeer { fd, token }))
}
