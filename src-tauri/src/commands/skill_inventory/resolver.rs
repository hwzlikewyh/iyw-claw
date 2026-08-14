use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use sea_orm::DatabaseConnection;

use crate::acp::error::AcpError;
use crate::db::service::plugin_installation_service;
use crate::models::agent::AgentType;

use super::types::{LogicalSkillInventoryItem, SkillAgentState};

pub(super) async fn apply_effective_states(
    conn: &DatabaseConnection,
    skills: &mut [LogicalSkillInventoryItem],
) -> Result<(), AcpError> {
    apply_plugin_availability(conn, skills).await?;
    reset_agent_states(skills);
    let agents = observed_agents(skills);
    let dependencies = dependency_ids(skills);
    for agent_type in agents {
        resolve_agent(skills, agent_type, &dependencies);
    }
    for skill in skills {
        skill.agent_states.sort_by_key(|state| state.agent_type);
    }
    Ok(())
}

async fn apply_plugin_availability(
    conn: &DatabaseConnection,
    skills: &mut [LogicalSkillInventoryItem],
) -> Result<(), AcpError> {
    let installations = plugin_installation_service::list_installations(conn)
        .await
        .map_err(|error| AcpError::protocol(error.to_string()))?;
    let available = installations
        .into_iter()
        .filter(|value| matches!(value.status.as_str(), "installed" | "degraded"))
        .filter(|value| Path::new(&value.install_root).is_dir())
        .map(|value| value.slug)
        .collect::<BTreeSet<_>>();
    for skill in skills {
        let primary = super::group::primary_observations(&skill.observations);
        let plugin_slugs = primary
            .iter()
            .filter_map(|value| value.plugin_slug.as_ref())
            .collect::<BTreeSet<_>>();
        skill.plugin_available = plugin_slugs
            .iter()
            .all(|plugin_slug| available.contains(*plugin_slug));
        skill.stale_market_record |= !plugin_slugs.is_empty() && !skill.plugin_available;
    }
    Ok(())
}

fn reset_agent_states(skills: &mut [LogicalSkillInventoryItem]) {
    for state in skills.iter_mut().flat_map(|skill| &mut skill.agent_states) {
        state.effective_enabled = false;
        state.required_by.clear();
        state.blocked_reasons.clear();
    }
}

fn observed_agents(skills: &[LogicalSkillInventoryItem]) -> BTreeSet<AgentType> {
    skills
        .iter()
        .flat_map(|skill| skill.agent_states.iter().map(|state| state.agent_type))
        .collect()
}

fn dependency_ids(skills: &[LogicalSkillInventoryItem]) -> BTreeSet<String> {
    skills
        .iter()
        .flat_map(|skill| skill.dependencies.iter().cloned())
        .collect()
}

fn resolve_agent(
    skills: &mut [LogicalSkillInventoryItem],
    agent_type: AgentType,
    dependency_ids: &BTreeSet<String>,
) {
    let index = global_skill_index(skills);
    let roots = desired_roots(skills, agent_type, dependency_ids);
    for root_index in roots {
        let root_id = skills[root_index].skill_id.clone();
        let mut closure = BTreeSet::new();
        let mut visiting = BTreeSet::new();
        let mut blockers = Vec::new();
        collect_closure(
            skills,
            root_index,
            agent_type,
            &index,
            &mut visiting,
            &mut closure,
            &mut blockers,
        );
        if blockers.is_empty() {
            enable_closure(skills, agent_type, root_index, &root_id, &closure);
        } else {
            state_mut(&mut skills[root_index], agent_type)
                .blocked_reasons
                .extend(blockers);
        }
    }
}

fn global_skill_index(skills: &[LogicalSkillInventoryItem]) -> BTreeMap<String, usize> {
    skills
        .iter()
        .enumerate()
        .filter(|(_, skill)| skill.scope == crate::acp::types::AgentSkillScope::Global)
        .map(|(index, skill)| (skill.skill_id.clone(), index))
        .collect()
}

fn desired_roots(
    skills: &[LogicalSkillInventoryItem],
    agent_type: AgentType,
    dependency_ids: &BTreeSet<String>,
) -> Vec<usize> {
    skills
        .iter()
        .enumerate()
        .filter(|(_, skill)| {
            state(skill, agent_type).is_some_and(|value| {
                value
                    .requested_enabled
                    .unwrap_or(value.actual_enabled && !dependency_ids.contains(&skill.skill_id))
            })
        })
        .map(|(index, _)| index)
        .collect()
}

fn collect_closure(
    skills: &[LogicalSkillInventoryItem],
    skill_index: usize,
    agent_type: AgentType,
    index: &BTreeMap<String, usize>,
    visiting: &mut BTreeSet<String>,
    closure: &mut BTreeSet<usize>,
    blockers: &mut Vec<String>,
) {
    let skill = &skills[skill_index];
    if let Some(reason) = blocked_reason(skill, agent_type) {
        push_unique(blockers, reason);
        return;
    }
    if !visiting.insert(skill.skill_id.clone()) {
        push_unique(blockers, format!("dependency_cycle:{}", skill.skill_id));
        return;
    }
    for dependency in &skill.dependencies {
        let Some(dependency_index) = index.get(dependency).copied() else {
            push_unique(blockers, format!("missing_dependency:{dependency}"));
            continue;
        };
        closure.insert(dependency_index);
        collect_closure(
            skills,
            dependency_index,
            agent_type,
            index,
            visiting,
            closure,
            blockers,
        );
    }
    visiting.remove(&skill.skill_id);
}

fn blocked_reason(skill: &LogicalSkillInventoryItem, agent_type: AgentType) -> Option<String> {
    if !agent_eligible(skill, agent_type) {
        Some(format!("agent_ineligible:{agent_type}"))
    } else if skill.conflict {
        Some(format!("conflict:{}", skill.skill_id))
    } else if skill.duplicate {
        Some(format!("duplicate:{}", skill.skill_id))
    } else if !skill.plugin_available {
        Some(format!("plugin_unavailable:{}", skill.skill_id))
    } else if skill.stale_market_record {
        Some(format!("stale_market_record:{}", skill.skill_id))
    } else if super::group::primary_observations(&skill.observations)
        .iter()
        .all(|value| value.content_tree_hash.is_none())
    {
        Some(format!("unreadable:{}", skill.skill_id))
    } else {
        None
    }
}

fn agent_eligible(skill: &LogicalSkillInventoryItem, agent_type: AgentType) -> bool {
    super::group::primary_observations(&skill.observations)
        .iter()
        .any(|observation| {
            observation.locations.iter().any(|location| {
                location.agent_types.contains(&agent_type)
                    || matches!(
                        observation.ownership,
                        super::types::SkillInventoryOwnership::IywManaged
                            | super::types::SkillInventoryOwnership::Market
                            | super::types::SkillInventoryOwnership::Plugin
                    )
            })
        })
}

fn enable_closure(
    skills: &mut [LogicalSkillInventoryItem],
    agent_type: AgentType,
    root_index: usize,
    root_id: &str,
    closure: &BTreeSet<usize>,
) {
    state_mut(&mut skills[root_index], agent_type).effective_enabled = true;
    for dependency_index in closure {
        let is_root = skills[*dependency_index].skill_id == root_id;
        let state = state_mut(&mut skills[*dependency_index], agent_type);
        state.effective_enabled = true;
        if !is_root {
            state.required_by.push(root_id.to_string());
            state.required_by.sort();
            state.required_by.dedup();
        }
    }
}

fn state(skill: &LogicalSkillInventoryItem, agent_type: AgentType) -> Option<&SkillAgentState> {
    skill
        .agent_states
        .iter()
        .find(|value| value.agent_type == agent_type)
}

fn state_mut(skill: &mut LogicalSkillInventoryItem, agent_type: AgentType) -> &mut SkillAgentState {
    if let Some(index) = skill
        .agent_states
        .iter()
        .position(|value| value.agent_type == agent_type)
    {
        return &mut skill.agent_states[index];
    }
    skill.agent_states.push(SkillAgentState {
        agent_type,
        requested_enabled: None,
        effective_enabled: false,
        actual_enabled: false,
        required_by: Vec::new(),
        blocked_reasons: Vec::new(),
        location_count: 0,
    });
    let index = skill.agent_states.len() - 1;
    &mut skill.agent_states[index]
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.contains(&value) {
        values.push(value);
    }
}
