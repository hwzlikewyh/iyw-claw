//! 原子持久化更新恢复清单；不保存凭据或完整提示词。
use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::acp::update_tasks::UpdateTask;

#[derive(Default, Serialize, Deserialize)]
pub(super) struct Recovery {
    pub tasks: Vec<UpdateTask>,
}

fn path() -> PathBuf {
    crate::system_skills::data_dir_from_env().join("inventory/update-task-recovery.json")
}

pub(super) fn load() -> Result<Recovery, String> {
    match std::fs::read(path()) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| error.to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Recovery::default()),
        Err(error) => Err(error.to_string()),
    }
}

pub(super) fn save(recovery: &Recovery) -> Result<(), String> {
    let path = path();
    let parent = path.parent().ok_or("Recovery directory is unavailable")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let bytes = serde_json::to_vec(recovery).map_err(|error| error.to_string())?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
    file.write_all(&bytes).map_err(|error| error.to_string())?;
    file.as_file()
        .sync_all()
        .map_err(|error| error.to_string())?;
    file.persist(&path).map_err(|error| error.to_string())?;
    Ok(())
}
