// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Where the helper of a iyw-claw running as `exe` is, on macOS (`mac`) or
/// elsewhere.
pub(super) fn helper_for(exe: &Path, mac: bool) -> Option<PathBuf> {
    let dir = exe.parent()?;
    let contents = dir.parent().filter(|contents| {
        mac && dir.file_name().is_some_and(|n| n == "MacOS")
            && contents.file_name().is_some_and(|n| n == "Contents")
            && contents
                .parent()
                .and_then(Path::extension)
                .is_some_and(|e| e.eq_ignore_ascii_case("app"))
    });
    Some(match contents {
        Some(contents) => contents
            .join("Helpers")
            .join(HELPER_APP)
            .join("Contents")
            .join("MacOS")
            .join(helper_file_name()),
        None => dir.join(helper_file_name()),
    })
}

/// 桌面端执行当前主程序；仅独立服务端沿用 macOS helper app 副本。
pub(super) async fn helper_to_run() -> Result<PathBuf, BackendError> {
    let shipped = locate_helper_binary().ok_or_else(|| {
        if cfg!(feature = "tauri-runtime") {
            return BackendError::Unavailable("内置电脑操作主程序不可用，请修复应用安装".into());
        }
        BackendError::Unavailable(format!(
            "{} is missing from this installation",
            helper_file_name()
        ))
    })?;
    if cfg!(feature = "tauri-runtime") {
        return Ok(shipped);
    }
    #[cfg(target_os = "macos")]
    if let Some(app) = nested_helper_app(&shipped).map(Path::to_path_buf) {
        let home = crate::computer::helper::driver_proc::helper_data_dir().ok_or_else(|| {
            BackendError::Unavailable("no home directory for this account".into())
        })?;
        return tokio::task::spawn_blocking(move || copy_to_run(&app, &home))
            .await
            .map_err(|e| BackendError::Unavailable(format!("copying the helper: {e}")))?;
    }
    Ok(shipped)
}
