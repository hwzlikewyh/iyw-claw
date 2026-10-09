// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Bring the copy of the helper app `shipped` in `home` up to date, and say
/// where its executable is: with every link followed, as the kernel will
/// find it, and in no app but its own — inside another, its Screen Recording
/// would be that app's again.
#[cfg(target_os = "macos")]
pub(super) fn copy_to_run(shipped: &Path, home: &Path) -> Result<PathBuf, BackendError> {
    let failed = |e: std::io::Error| {
        BackendError::Unavailable(format!("could not copy {HELPER_APP} to run: {e}"))
    };
    let installed = crate::computer::helper_app::install(shipped, home).map_err(failed)?;
    let exe = std::fs::canonicalize(
        installed
            .join("Contents")
            .join("MacOS")
            .join(helper_file_name()),
    )
    .map_err(failed)?;
    if !alone_in_its_app(&exe) {
        return Err(BackendError::Unavailable(format!(
            "the copy of {HELPER_APP} to run is at {}, not in an app of its own",
            exe.display()
        )));
    }
    Ok(exe)
}
