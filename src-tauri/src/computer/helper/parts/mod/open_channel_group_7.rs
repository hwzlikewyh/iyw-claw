// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Decide who is on the other end of stdin, before anything is read from it.
#[cfg(target_os = "macos")]
pub(super) fn open_channel() -> Result<Channel, (i32, String)> {
    use crate::computer::codesign::{check_guest, peer_audit_token, self_info, Guest};

    let requirement = PEER_REQUIREMENT.filter(|r| !r.trim().is_empty());
    if requirement.is_none() {
        // A helper that cannot tell whether it is a release build is treated
        // as one.
        let me = self_info().map_err(|e| {
            (
                EXIT_UNANCHORED,
                format!("cannot read my own signature: {e}"),
            )
        })?;
        if me.team_id.is_some() {
            return Err((
                EXIT_UNANCHORED,
                "this helper is signed with a Team ID but was built without iyw-claw's \
                 designated requirement; it would serve any caller"
                    .into(),
            ));
        }
    }

    if !is_socket(0) {
        return match requirement {
            Some(_) => Err((EXIT_PEER_REFUSED, "stdin is not a socket".into())),
            None => {
                tracing::warn!("development build: serving plain stdio without a peer check");
                Ok(Channel {
                    raw: RawChannel::Stdio,
                    peer: PeerCheck::Development,
                    guard: None,
                })
            }
        };
    }
    let (peer, guard) = match requirement {
        Some(requirement) => {
            // Replies go back down the socket the requests came in on, never
            // to a descriptor someone else wired up as stdout.
            if !same_file(0, 1) {
                return Err((EXIT_PEER_REFUSED, "stdout is not the stdin socket".into()));
            }
            let token = peer_audit_token(0)
                .map_err(|e| (EXIT_PEER_REFUSED, format!("no peer token: {e}")))?;
            // The iyw-claw this helper serves is the one that launched it: the
            // peer must be this process's parent. Otherwise a process could
            // get a genuine iyw-claw to write once into a socket of its own
            // making and start the helper on the other end — the token would
            // name that iyw-claw, and the helper would serve whoever started it.
            // SAFETY: getppid cannot fail.
            let parent = unsafe { libc::getppid() };
            if i64::from(token.pid()) != i64::from(parent) {
                return Err((
                    EXIT_PEER_REFUSED,
                    format!(
                        "the peer (pid {}) is not the process that launched this helper \
                         (pid {parent})",
                        token.pid()
                    ),
                ));
            }
            let info = check_guest(Guest::Audit(token), requirement)
                .map_err(|e| (EXIT_PEER_REFUSED, format!("the peer is not iyw-claw: {e}")))?;
            info.entitlements_clean().map_err(|e| {
                (
                    EXIT_PEER_REFUSED,
                    format!("the peer is not a iyw-claw this helper serves: {e}"),
                )
            })?;
            (PeerCheck::Verified, Some(PeerGuard { token }))
        }
        None => {
            tracing::warn!("development build: serving without checking the peer's signature");
            (PeerCheck::Development, None)
        }
    };
    // SAFETY: fd 0 is a socket (checked above) that this process owns for its
    // whole life; nothing else in the helper touches descriptor 0.
    let socket = unsafe {
        use std::os::fd::FromRawFd;
        std::os::unix::net::UnixStream::from_raw_fd(0)
    };
    Ok(Channel {
        raw: RawChannel::Socket(socket),
        peer,
        guard,
    })
}
