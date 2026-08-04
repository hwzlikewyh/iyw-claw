use std::path::PathBuf;

pub(super) fn managed_git_bin_dir() -> Option<PathBuf> {
    crate::acp::version_center::managed_tool_executable("git")?
        .parent()
        .map(ToOwned::to_owned)
}
