use std::fs;
use std::path::{Path, PathBuf};

use crate::acp::agent_storage::AgentStoragePaths;
use crate::commands::experts::central_experts_dir;

use super::*;

pub(super) fn find_agent_reach_skill(paths: &AgentStoragePaths) -> Option<PathBuf> {
    walkdir::WalkDir::new(paths.uv_runtime_dir().join("tools"))
        .max_depth(8)
        .into_iter()
        .filter_map(Result::ok)
        .map(|entry| entry.into_path())
        .find(|path| {
            path.file_name().and_then(|name| name.to_str()) == Some("skill")
                && path.join("SKILL.md").is_file()
                && path
                    .parent()
                    .and_then(Path::file_name)
                    .and_then(|name| name.to_str())
                    == Some("agent_reach")
        })
}

fn copy_dir(source: &Path, target: &Path) -> Result<(), String> {
    if target.exists() || fs::symlink_metadata(target).is_ok() {
        crate::commands::acp::remove_skill_entry(target).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(target).map_err(|error| error.to_string())?;
    for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let destination = target.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &destination)?;
        } else {
            fs::copy(entry.path(), destination).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

pub(super) fn sync_packaged_skills(
    agent_reach_skill: &Path,
    opencli_skills: &Path,
    central: &Path,
) -> Result<Vec<String>, String> {
    fs::create_dir_all(central).map_err(|error| error.to_string())?;
    copy_dir(agent_reach_skill, &central.join("agent-reach"))?;
    let mut synced = vec!["agent-reach".to_string()];
    sync_opencli_skills(opencli_skills, central, &mut synced)?;
    Ok(synced)
}

pub(super) fn sync_installed_skills(paths: &AgentStoragePaths) -> Result<Vec<String>, String> {
    let central = central_experts_dir();
    let mut synced = Vec::new();
    if let Some(agent_reach_skill) = find_agent_reach_skill(paths) {
        copy_dir(&agent_reach_skill, &central.join("agent-reach"))?;
        synced.push("agent-reach".to_string());
    }
    let opencli_skills = opencli_prefix(paths).join("node_modules/@jackwener/opencli/skills");
    if opencli_skills.is_dir() {
        fs::create_dir_all(&central).map_err(|error| error.to_string())?;
        sync_opencli_skills(&opencli_skills, &central, &mut synced)?;
    }
    synced.sort();
    Ok(synced)
}

fn sync_opencli_skills(
    source: &Path,
    central: &Path,
    synced: &mut Vec<String>,
) -> Result<(), String> {
    for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let id = entry.file_name().to_string_lossy().to_string();
        if !id.starts_with("opencli-") || !entry.path().join("SKILL.md").is_file() {
            continue;
        }
        copy_dir(&entry.path(), &central.join(&id))?;
        synced.push(id);
    }
    Ok(())
}

pub(super) fn list_internet_skills_from(central: &Path) -> Vec<InternetToolSkill> {
    let Ok(entries) = fs::read_dir(central) else {
        return Vec::new();
    };
    let mut skills = entries
        .flatten()
        .filter_map(|entry| {
            let id = entry.file_name().to_string_lossy().to_string();
            let source = if id == "agent-reach" {
                InternetToolId::AgentReach
            } else if id.starts_with("opencli-") {
                InternetToolId::Opencli
            } else {
                return None;
            };
            entry
                .path()
                .join("SKILL.md")
                .is_file()
                .then_some(InternetToolSkill {
                    id,
                    source,
                    installed_centrally: true,
                })
        })
        .collect::<Vec<_>>();
    skills.sort_by(|left, right| left.id.cmp(&right.id));
    skills
}

pub(super) fn uninstall_targets(
    paths: &AgentStoragePaths,
    tool: InternetToolId,
    remove_config: bool,
    agent_reach_config: &Path,
) -> Vec<PathBuf> {
    let mut targets = match tool {
        InternetToolId::AgentReach => vec![paths.uv_runtime_dir().join("tools/agent-reach")],
        InternetToolId::Opencli => vec![opencli_prefix(paths)],
    };
    if tool == InternetToolId::AgentReach && remove_config {
        targets.push(agent_reach_config.to_path_buf());
    }
    targets
}

pub(super) fn agent_reach_config_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".agent-reach"))
}

pub(super) fn remove_managed_skill(skill_id: &str) -> Result<(), String> {
    let path = central_experts_dir().join(skill_id);
    if path.exists() || fs::symlink_metadata(&path).is_ok() {
        crate::commands::acp::remove_skill_entry(&path).map_err(|error| error.to_string())?;
    }
    Ok(())
}
