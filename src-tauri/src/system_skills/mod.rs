mod activation;
mod checkout;
pub mod git;
pub mod manager;
pub mod manifest;
pub mod state;
mod storage;

pub use manager::{
    apply_update_core, check_update_core, rollback_core, snapshot_core, startup_update_core,
};
pub use state::{SystemSkillsUpdateState, SYSTEM_SKILLS_UPDATE_EVENT};

pub const REPOSITORY_URL: &str = "https://gitlab.iyw.cn/hwz/skill.git";

/// Built-in account used to fetch the system skill repository when the install
/// has no account configured for its host.
///
/// This credential ships inside the desktop binary and is therefore recoverable
/// from any copy of it, so it must stay scoped to read-only access on
/// `hwz/skill` and nothing else. A configured account for the same host always
/// wins (see `git::inject_system_skills_credentials`), so a deployment can
/// rotate away from this one without shipping a new build.
pub const BUILTIN_USERNAME: &str = "iyw_lq";
pub const BUILTIN_PASSWORD: &str = "iyw@123456789";

pub fn repository_dir() -> std::path::PathBuf {
    crate::commands::experts::central_experts_dir().join(".system-repo")
}

pub fn staging_dir() -> std::path::PathBuf {
    crate::commands::experts::central_experts_dir().join(".system-repo.staging")
}

pub fn data_dir_from_env() -> std::path::PathBuf {
    std::env::var_os("IYW_CLAW_DATA_DIR")
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(crate::paths::iyw_claw_home_dir)
}
