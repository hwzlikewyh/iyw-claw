// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl ExitWatch {
    pub(in crate::computer::spawn) fn register(pid: libc::pid_t) -> std::io::Result<Self> {
        // SAFETY: plain kqueue calls on a descriptor this value owns (closed
        // on drop); `change` is valid for the call that reads it.
        unsafe {
            let kq = libc::kqueue();
            if kq < 0 {
                return Err(std::io::Error::last_os_error());
            }
            let watch = Self { kq, exited: false };
            let change = libc::kevent {
                ident: pid as libc::uintptr_t,
                filter: libc::EVFILT_PROC,
                flags: libc::EV_ADD | libc::EV_ONESHOT,
                fflags: libc::NOTE_EXIT,
                data: 0,
                udata: std::ptr::null_mut(),
            };
            if libc::kevent(kq, &change, 1, std::ptr::null_mut(), 0, std::ptr::null()) == 0 {
                return Ok(watch);
            }
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::ESRCH) {
                return Ok(Self {
                    exited: true,
                    ..watch
                });
            }
            Err(err)
        }
    }

    /// Block until the child has exited, or the queue fails — after which the
    /// caller polls instead.
    pub(in crate::computer::spawn) fn wait(&self) {
        if self.exited {
            return;
        }
        // SAFETY: `event` is a valid out-parameter for the call.
        unsafe {
            let mut event: libc::kevent = std::mem::zeroed();
            loop {
                let n = libc::kevent(
                    self.kq,
                    std::ptr::null(),
                    0,
                    &mut event,
                    1,
                    std::ptr::null(),
                );
                if n > 0 {
                    return;
                }
                if n < 0
                    && std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted
                {
                    return;
                }
            }
        }
    }
}

impl Drop for ExitWatch {
    fn drop(&mut self) {
        // SAFETY: closing the descriptor this value owns, once.
        unsafe { libc::close(self.kq) };
    }
}
