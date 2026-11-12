use iyw_share_core::{invalid, Result};
use std::{path::Path, process::Command, time::Duration};

const CREATE_NO_WINDOW: u32 = 0x08000000;
const STATUS_TIMEOUT: Duration = Duration::from_secs(15);
const STATUS_POLL_INTERVAL: Duration = Duration::from_millis(50);
const REGISTER_TIMEOUT: Duration = Duration::from_secs(60);
static REGISTRATION: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn command(root: &Path) -> Result<Command> {
    use std::os::windows::process::CommandExt;
    let script = root.join("register.ps1");
    if !script.is_file() {
        return Err(invalid("当前安装包没有系统分享注册组件"));
    }
    let system_root =
        std::env::var_os("SystemRoot").ok_or_else(|| invalid("无法定位系统 Windows PowerShell"))?;
    let executable = Path::new(&system_root).join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let mut command = Command::new(executable);
    command
        .env_remove("PSModulePath")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(script)
        .creation_flags(CREATE_NO_WINDOW);
    Ok(command)
}

pub fn installed(root: &Path) -> bool {
    use std::process::Stdio;
    let Ok(mut command) = command(root) else {
        return false;
    };
    let Ok(mut child) = command
        .arg("-CheckOnly")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    let started = std::time::Instant::now();
    while started.elapsed() < STATUS_TIMEOUT {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) => std::thread::sleep(STATUS_POLL_INTERVAL),
            Err(_) => break,
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    tracing::warn!(target: "iyw_native_share", "native share status check did not complete");
    false
}

pub async fn register(root: &Path) -> Result<()> {
    let _guard = REGISTRATION.lock().await;
    let mut command = tokio::process::Command::from(command(root)?);
    command.kill_on_drop(true);
    let output = tokio::time::timeout(REGISTER_TIMEOUT, command.output())
        .await
        .map_err(|_| invalid("系统分享入口注册超时"))??;
    if !output.status.success() {
        let reason = String::from_utf8_lossy(&output.stderr);
        tracing::warn!(target: "iyw_native_share", code = ?output.status.code(), reason = %reason.trim(), "share package registration failed");
        return Err(invalid("系统分享入口注册失败，具体原因已记录在应用日志中"));
    }
    Ok(())
}
