// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl Child {
    pub(in crate::computer::spawn) fn new(pid: u32) -> std::io::Result<Self> {
        let raw = pid as libc::pid_t;
        let watch = match ExitWatch::register(raw) {
            Ok(watch) => watch,
            Err(e) => {
                discard(raw);
                return Err(e);
            }
        };
        let state = Arc::new(Mutex::new(ChildState::default()));
        let (tx, rx) = tokio::sync::watch::channel(false);
        let waiter_state = state.clone();
        let spawned = std::thread::Builder::new()
            .name(format!("computer-child-{pid}"))
            .spawn(move || {
                watch.wait();
                loop {
                    {
                        let mut state = waiter_state.lock().unwrap_or_else(|p| p.into_inner());
                        let mut status: c_int = 0;
                        // SAFETY: collecting our own child. `WNOHANG`: under
                        // this lock a signal cannot be in flight to the pid
                        // being collected, and nothing waits here while the
                        // child still runs.
                        let rc = unsafe { libc::waitpid(raw, &mut status, libc::WNOHANG) };
                        if rc == raw {
                            state.reaped = true;
                            state.exit_status = Some(if libc::WIFEXITED(status) {
                                libc::WEXITSTATUS(status)
                            } else {
                                -libc::WTERMSIG(status)
                            });
                            break;
                        }
                        if rc < 0
                            && std::io::Error::last_os_error().kind()
                                != std::io::ErrorKind::Interrupted
                        {
                            // Not ours to collect any more: the pid must not be
                            // signalled again.
                            state.reaped = true;
                            break;
                        }
                    }
                    std::thread::sleep(REAP_POLL);
                }
                let _ = tx.send(true);
            });
        if let Err(e) = spawned {
            discard(raw);
            return Err(e);
        }
        Ok(Self {
            pid,
            state,
            exited: rx,
        })
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub(in crate::computer::spawn) fn signal(&self, sig: c_int) -> std::io::Result<()> {
        let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.reaped {
            return Err(std::io::Error::other("the child has already exited"));
        }
        // SAFETY: the pid is our unreaped child (checked under the lock the
        // waiter takes before reaping).
        let rc = unsafe { libc::kill(self.pid as libc::pid_t, sig) };
        if rc == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }

    /// Let a suspended child run.
    pub fn resume(&self) -> std::io::Result<()> {
        self.signal(libc::SIGCONT)
    }

    /// Ask the child to stop.
    pub fn terminate(&self) {
        let _ = self.signal(libc::SIGTERM);
    }

    /// Stop the child outright. A suspended child ignores nothing but this and
    /// `SIGCONT`, which is why a child that failed its checks is killed rather
    /// than asked.
    pub fn kill(&self) {
        let _ = self.signal(libc::SIGKILL);
    }

    pub fn has_exited(&self) -> bool {
        *self.exited.borrow()
    }

    /// Wait for the child to exit and be collected.
    pub async fn wait(&self) -> Option<i32> {
        let mut rx = self.exited.clone();
        let _ = rx.wait_for(|done| *done).await;
        self.state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .exit_status
    }
}
