//! OfficeCLI 命令解释器、文档引擎和技能随主程序链接，不安装外部 CLI。
use std::ffi::OsStr;
use std::path::PathBuf;
use std::sync::Mutex;

static SHIM_LOCK: Mutex<()> = Mutex::new(());
pub const VERSION: &str = "0.1.31";

/// 在 GUI/服务端启动之前处理办公命令，避免初始化应用或连接业务服务。
pub fn dispatch_early() -> bool {
    let mut arguments = std::env::args_os();
    let _ = arguments.next();
    if arguments.next().as_deref() != Some(OsStr::new("--embedded-office")) {
        return false;
    }
    let mut args = vec!["officecli".into()];
    args.extend(arguments);
    iyw_office_engine::run(args);
    true
}

pub(crate) fn skill(name: &str) -> Result<String, String> {
    iyw_office_engine::bundled_skill(name).map_err(|error| error.to_string())
}

pub(crate) fn agent_entrypoint() -> Result<PathBuf, String> {
    let _guard = SHIM_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let root = crate::paths::iyw_claw_user_dir()
        .join("runtime")
        .join("embedded-office");
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let name = if cfg!(windows) {
        "officecli.cmd"
    } else {
        "officecli"
    };
    let target = root.join(name);
    let path = executable.to_string_lossy();
    let script = if cfg!(windows) {
        format!(
            "@echo off\r\nsetlocal DisableDelayedExpansion\r\n\"{}\" --embedded-office %*\r\n",
            path.replace('%', "%%")
        )
    } else {
        format!(
            "#!/bin/sh\nexec '{}' --embedded-office \"$@\"\n",
            path.replace('\'', "'\\''")
        )
    };
    if std::fs::read_to_string(&target).ok().as_deref() != Some(&script) {
        let mut staged =
            tempfile::NamedTempFile::new_in(&root).map_err(|error| error.to_string())?;
        std::io::Write::write_all(&mut staged, script.as_bytes())
            .map_err(|error| error.to_string())?;
        staged.persist(&target).map_err(|error| error.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o700))
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(target)
}
