// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Kill and collect a child no handle will be given for. Blocking is fine: a
/// `SIGKILL` ends even a stopped child.
pub(super) fn discard(pid: libc::pid_t) {
    let mut status: c_int = 0;
    // SAFETY: our own unreaped child; nothing else holds its pid.
    unsafe {
        libc::kill(pid, libc::SIGKILL);
        libc::waitpid(pid, &mut status, 0);
    }
}

/// How often the waiter looks again when the exit it was told of has not
/// happened yet (a failed queue, or a spurious wakeup).
pub(super) const REAP_POLL: std::time::Duration = std::time::Duration::from_millis(50);
