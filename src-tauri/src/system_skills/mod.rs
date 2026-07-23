mod activation;
mod checkout;
pub mod git;
pub mod manager;
pub mod manifest;
pub mod state;
mod storage;

pub use manager::{
    apply_update_core, check_update_core, rollback_core, set_auto_update_core, snapshot_core,
    startup_update_core,
};
pub use state::{SystemSkillsUpdateState, SYSTEM_SKILLS_UPDATE_EVENT};

pub const REPOSITORY_URL: &str = "https://gitlab.iyw.cn/hwz/skill.git";

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
