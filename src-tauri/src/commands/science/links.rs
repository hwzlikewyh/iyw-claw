use std::path::PathBuf;

use crate::acp::types::AgentSkillScope;
use crate::commands::acp::{preferred_scope_skill_dir, scoped_skill_dirs};
use crate::commands::experts::{
    classify_link, managed_copy_is_owned, read_link_target, ExpertInstallStatus, LinkOpResult,
};
use crate::models::agent::AgentType;

use super::metadata::bundled_metadata;
use super::{central_path, ScienceError};

pub(super) fn managed_ids() -> Vec<String> {
    bundled_metadata()
        .iter()
        .map(|metadata| metadata.id.clone())
        .collect()
}

pub(super) fn managed_ready_ids() -> Vec<String> {
    bundled_metadata()
        .iter()
        .filter(|metadata| central_path(&metadata.id).exists())
        .map(|metadata| metadata.id.clone())
        .collect()
}

pub(super) fn has_owned_link(skill_id: &str, agents: &[AgentType]) -> bool {
    crate::commands::experts::managed_expert_has_owned_link(skill_id, agents)
}

pub(super) async fn reconcile(targets: &[(AgentType, String, bool)]) -> Vec<LinkOpResult> {
    crate::commands::experts::reconcile_managed_experts(targets).await
}

fn supported_agents() -> Vec<AgentType> {
    crate::commands::managed_skills::supported_skill_agent_types()
}

fn preferred_link_path(agent_type: AgentType, skill_id: &str) -> Result<PathBuf, ScienceError> {
    preferred_scope_skill_dir(agent_type, AgentSkillScope::Global, None)
        .map(|directory| directory.join(skill_id))
        .map_err(|error| ScienceError::Io(error.to_string()))
}

fn status_for(skill_id: &str, agent_type: AgentType) -> Result<ExpertInstallStatus, ScienceError> {
    let expected = central_path(skill_id);
    let preferred = preferred_link_path(agent_type, skill_id)?;
    let paths = scoped_skill_dirs(agent_type, AgentSkillScope::Global, None)
        .map_err(|error| ScienceError::Io(error.to_string()))?
        .into_iter()
        .map(|directory| directory.join(skill_id))
        .collect::<Vec<_>>();
    let link_path = paths
        .iter()
        .find(|path| crate::commands::experts::managed_link_is_owned(&expected, path))
        .cloned()
        .unwrap_or(preferred);
    Ok(ExpertInstallStatus {
        expert_id: skill_id.to_string(),
        agent_type,
        state: classify_link(&link_path, &expected),
        target_path: read_link_target(&link_path)
            .map(|target| target.to_string_lossy().to_string()),
        copy_mode: managed_copy_is_owned(&expected, &link_path),
        link_path: link_path.to_string_lossy().to_string(),
        expected_target_path: expected.to_string_lossy().to_string(),
    })
}

pub(super) fn list_all_install_statuses() -> Result<Vec<ExpertInstallStatus>, ScienceError> {
    let agents = supported_agents();
    let mut statuses = Vec::with_capacity(bundled_metadata().len() * agents.len());
    for metadata in bundled_metadata() {
        for agent_type in &agents {
            statuses.push(status_for(&metadata.id, *agent_type)?);
        }
    }
    Ok(statuses)
}
