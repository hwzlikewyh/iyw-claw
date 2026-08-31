pub mod manager;
pub mod manifest;
pub mod state;

pub use manager::{
    apply_update_core, check_update_core, rollback_core, snapshot_core, startup_update_core,
};
pub use state::{SystemSkillsUpdateState, SYSTEM_SKILLS_UPDATE_EVENT};

/// Legacy checkout path retained for old installs. It may remain the managed
/// source when a runtime environment must stay in place during a refresh.
pub fn repository_dir() -> std::path::PathBuf {
    crate::commands::experts::central_experts_dir().join(".system-repo")
}

pub fn data_dir_from_env() -> std::path::PathBuf {
    std::env::var_os("IYW_CLAW_DATA_DIR")
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(crate::paths::iyw_claw_home_dir)
}
