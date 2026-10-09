// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The audit token of the process at the other end of the AF_UNIX socket
/// `fd`.
///
/// For a socketpair this is the last process to have used the *other* end.
/// Both ends of a socketpair start out belonging to whoever created it, so a
/// peer is only meaningful once that other end has been handed over and used
/// — see `protocol`'s note on why the helper speaks first.
pub fn peer_audit_token(fd: RawFd) -> std::io::Result<AuditToken> {
    let mut token = [0u32; 8];
    let mut len = std::mem::size_of_val(&token) as libc::socklen_t;
    // SAFETY: `token` is a 32-byte buffer and `len` says so; the kernel writes
    // at most `len` bytes and reports how many.
    let rc = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_LOCAL,
            libc::LOCAL_PEERTOKEN,
            token.as_mut_ptr().cast(),
            &mut len,
        )
    };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    if len as usize != std::mem::size_of_val(&token) {
        return Err(std::io::Error::other("short LOCAL_PEERTOKEN"));
    }
    Ok(AuditToken(token))
}

/// Which running process to check.
#[derive(Debug, Clone, Copy)]
pub enum Guest {
    /// A socket peer.
    Audit(AuditToken),
    /// Our own suspended, unreaped child.
    ChildPid(u32),
}

/// What a signature says about a running process, once it has checked out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodeInfo {
    pub identifier: Option<String>,
    pub team_id: Option<String>,
    /// The code-directory hash, lowercase hex.
    pub cdhash: Option<String>,
    /// The code directory's flags (`0x10000` is the hardened runtime).
    pub flags: Option<u32>,
    /// Every entitlement the signature carries that is not a boolean `false`.
    pub entitlements: Vec<String>,
}
