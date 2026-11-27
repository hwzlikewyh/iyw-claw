//! 安装向导使用已解包的 AppImage。更新时原子交换 app 目录，保留旁边的品牌和用户数据。
use std::{
    ffi::CString,
    fs, io,
    os::unix::{ffi::OsStrExt, fs::PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
};

use std::io::Result;

pub(super) fn managed_app_dir() -> Result<Option<PathBuf>> {
    let executable = std::env::current_exe()?;
    let Some(root) = executable.ancestors().nth(4) else {
        return Ok(None);
    };
    let app = root.join("app");
    if executable != app.join("usr/bin/iyw-claw")
        || !(root.join("installed-release.json").is_file()
            || root.join("config/branding.json").is_file())
    {
        return Ok(None);
    }
    Ok(Some(app))
}

pub(super) fn install(
    app: &Path,
    bytes: &[u8],
    update: &tauri_plugin_updater::Update,
) -> Result<()> {
    let ext = super::release::extensions(update).map_err(io::Error::other)?;
    let digest = ext
        .sha256
        .ok_or_else(|| io::Error::other("更新制品缺少摘要"))?;
    super::verify::verify_release_signature(bytes, &update.signature).map_err(io::Error::other)?;
    let root = app
        .parent()
        .ok_or_else(|| io::Error::other("缺少应用安装目录"))?;
    let stage = tempfile::Builder::new()
        .prefix(".iyw-update-")
        .tempdir_in(root)?;
    let archive = stage.path().join("update.AppImage");
    fs::write(&archive, bytes)?;
    fs::set_permissions(&archive, fs::Permissions::from_mode(0o700))?;
    let status = Command::new(&archive)
        .arg("--appimage-extract")
        .current_dir(stage.path())
        .stdout(std::process::Stdio::null())
        .status()?;
    if !status.success() {
        return Err(io::Error::other(format!(
            "解包签名的 AppImage 失败：{status}"
        )));
    }
    let incoming = stage.path().join("squashfs-root");
    for entry in ["AppRun", "usr/bin/iyw-claw", "usr/bin/iyw-environment"] {
        if !incoming.join(entry).is_file() {
            return Err(io::Error::other(format!("更新包缺少 {entry}")));
        }
    }
    fs::write(
        incoming.join(".iyw-installed-release"),
        format!("{}\n{digest}\n", update.version),
    )?;
    exchange(app, &incoming)?;
    tracing::info!(target: "iyw_claw_update", "已切换本品牌的 Linux 应用目录");
    Ok(())
}

fn exchange(current: &Path, incoming: &Path) -> Result<()> {
    let current = CString::new(current.as_os_str().as_bytes())?;
    let incoming = CString::new(incoming.as_os_str().as_bytes())?;
    // 同一文件系统上的目录交换。失败时旧目录仍位于原处。
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            current.as_ptr(),
            libc::AT_FDCWD,
            incoming.as_ptr(),
            libc::RENAME_EXCHANGE,
        )
    };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
