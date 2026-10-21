use std::ffi::OsString;

use tauri_plugin_updater::UpdaterBuilder;

use crate::app_error::AppCommandError;

pub(super) fn retain_install_root(
    builder: UpdaterBuilder,
) -> Result<UpdaterBuilder, AppCommandError> {
    let executable = std::env::current_exe().map_err(AppCommandError::io)?;
    let Some(root) = crate::desktop_bootstrap::resolve_install_root(&executable) else {
        return Ok(builder);
    };
    // NSIS 要求 /D= 位于最后且不加引号，即使目录包含空格。
    let mut argument = OsString::from("/D=");
    argument.push(&root);
    tracing::debug!(
        install_root = %root.display(),
        "[app-update] retaining installation directory"
    );
    Ok(builder.installer_arg(argument))
}
