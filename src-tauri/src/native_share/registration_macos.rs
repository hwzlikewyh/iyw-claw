use iyw_share_core::{invalid, Result};
use std::{
    path::PathBuf,
    process::Stdio,
    time::{Duration, Instant},
};

const EXTENSION_ID: &str = "app.iywclaw.share";
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const PROBE_INTERVAL: Duration = Duration::from_millis(50);

pub fn extension_path() -> Result<PathBuf> {
    std::env::current_exe()?
        .parent()
        .and_then(|path| path.parent())
        .map(|path| path.join("PlugIns/IywAssistantShare.appex"))
        .ok_or_else(|| invalid("无法定位原助理分享扩展"))
}

fn run(arguments: &[&std::ffi::OsStr]) -> Result<String> {
    let mut child = std::process::Command::new("/usr/bin/pluginkit")
        .args(arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let deadline = Instant::now() + PROBE_TIMEOUT;
    while child.try_wait()?.is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(invalid("系统分享扩展注册服务超时"));
        }
        std::thread::sleep(PROBE_INTERVAL);
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        tracing::warn!(target: "iyw_native_share", code = ?output.status.code(), "pluginkit operation failed");
        return Err(invalid("无法查询或启用系统分享扩展，请检查系统设置"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn election() -> Result<Option<char>> {
    let output = run(&["-m".as_ref(), "-i".as_ref(), EXTENSION_ID.as_ref()])?;
    Ok(output
        .lines()
        .find(|line| {
            line.split_whitespace()
                .any(|part| part.split('(').next() == Some(EXTENSION_ID))
        })
        .and_then(|line| line.chars().next()))
}

pub fn status() -> Result<super::ShareStatus> {
    let can_register = extension_path()?.is_dir();
    let installed = can_register && election()? == Some('+');
    Ok(super::ShareStatus {
        platform: "macos",
        installed,
        can_register,
    })
}

pub fn register() -> Result<()> {
    let path = extension_path()?;
    if !path.is_dir() {
        return Err(invalid("当前应用没有内嵌分享扩展，请重新安装正式应用"));
    }
    run(&["-a".as_ref(), path.as_os_str()])?;
    run(&[
        "-e".as_ref(),
        "use".as_ref(),
        "-i".as_ref(),
        EXTENSION_ID.as_ref(),
    ])?;
    if !status()?.installed {
        return Err(invalid(
            "系统尚未启用原助理分享扩展，请在系统设置的共享扩展中启用",
        ));
    }
    Ok(())
}
