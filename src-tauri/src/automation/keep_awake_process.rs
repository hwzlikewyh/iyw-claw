use std::io::{self, Read};
use std::process::{Child, Command, Stdio};

pub(super) struct Inhibitor(Child);

impl Inhibitor {
    pub(super) fn start() -> io::Result<Self> {
        let mut command = inhibitor_command()?;
        let mut guard = Self(
            command
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()?,
        );
        guard.refresh()?;
        Ok(guard)
    }

    pub(super) fn refresh(&mut self) -> io::Result<()> {
        if let Some(status) = self.0.try_wait()? {
            let mut detail = String::new();
            if let Some(stderr) = self.0.stderr.take() {
                // 只保留系统命令自身的诊断，限制一次日志大小。
                let _ = stderr
                    .take(MAX_DIAGNOSTIC_BYTES)
                    .read_to_string(&mut detail);
            }
            return Err(io::Error::other(format!(
                "sleep inhibitor exited with {status}: {}",
                detail.trim()
            )));
        }
        Ok(())
    }
}

const MAX_DIAGNOSTIC_BYTES: u64 = 4096;

#[cfg(target_os = "macos")]
fn inhibitor_command() -> io::Result<Command> {
    let mut command = Command::new("/usr/bin/caffeinate");
    // 即使客户端被强制结束，系统监视的 PID 退出后也会释放锁。
    command.args(["-i", "-w", &std::process::id().to_string()]);
    Ok(command)
}

#[cfg(target_os = "linux")]
fn inhibitor_command() -> io::Result<Command> {
    let mut command = Command::new("systemd-inhibit");
    // cat 等待由客户端持有的管道；退出或崩溃关闭管道后抑制锁自动释放。
    command.args([
        "--what=idle:sleep",
        "--who=iyw-claw",
        "--why=Scheduled automations are enabled",
        "--mode=block",
        "--no-ask-password",
        "/bin/cat",
    ]);
    Ok(command)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn inhibitor_command() -> io::Result<Command> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "sleep inhibition is unavailable on this platform",
    ))
}

impl Drop for Inhibitor {
    fn drop(&mut self) {
        self.0.stdin.take();
        match self.0.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => {
                let _ = self.0.kill();
            }
            Err(error) => {
                tracing::warn!(error = %error, "[automation] sleep inhibitor status check failed");
            }
        }
        if let Err(error) = self.0.wait() {
            tracing::warn!(error = %error, "[automation] sleep inhibitor reap failed");
        }
    }
}
