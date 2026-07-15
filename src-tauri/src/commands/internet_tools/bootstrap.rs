use std::fs;
use std::path::Path;

use crate::acp::agent_storage::AgentStoragePaths;
use crate::commands::experts::central_experts_dir;

use super::*;

fn bootstrap_is_complete(paths: &AgentStoragePaths) -> bool {
    let opencli_skills = opencli_prefix(paths).join("node_modules/@jackwener/opencli/skills");
    fs::read_to_string(paths.root().join(BOOTSTRAP_MARKER))
        .ok()
        .as_deref()
        == Some(bootstrap_marker_content().as_str())
        && agent_reach_command_path(paths).is_file()
        && opencli_command_path(paths).is_file()
        && npm_tool_command_path(paths, "mcporter").is_file()
        && mcporter_config_path(paths).is_file()
        && central_experts_dir().join("agent-reach/SKILL.md").is_file()
        && packaged_opencli_skills_complete(&opencli_skills, &central_experts_dir())
}

fn packaged_opencli_skills_complete(source: &Path, central: &Path) -> bool {
    let Ok(entries) = fs::read_dir(source) else {
        return false;
    };
    let mut found = false;
    for entry in entries.flatten() {
        let id = entry.file_name().to_string_lossy().to_string();
        if !id.starts_with("opencli-") || !entry.path().join("SKILL.md").is_file() {
            continue;
        }
        found = true;
        if !central.join(id).join("SKILL.md").is_file() {
            return false;
        }
    }
    found
}

pub async fn bootstrap_core() -> Result<usize, String> {
    let _guard = bootstrap_lock().lock().await;
    let Some(paths) = AgentStoragePaths::active() else {
        return Ok(0);
    };
    if bootstrap_is_complete(&paths) {
        return Ok(0);
    }
    install_agent_reach(&paths).await?;
    install_opencli(&paths).await?;
    let agent_reach_skill = find_agent_reach_skill(&paths)
        .ok_or_else(|| "Agent Reach packaged skill was not found".to_string())?;
    let opencli_skills = opencli_prefix(&paths).join("node_modules/@jackwener/opencli/skills");
    let synced = sync_packaged_skills(&agent_reach_skill, &opencli_skills, &central_experts_dir())?;
    fs::write(
        paths.root().join(BOOTSTRAP_MARKER),
        bootstrap_marker_content(),
    )
    .map_err(|error| error.to_string())?;
    Ok(synced.len())
}
