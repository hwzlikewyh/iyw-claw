use std::collections::{BTreeMap, BTreeSet};

use crate::models::agent::AgentType;

use super::types::{
    LogicalSkillInventoryItem, SkillAgentState, SkillInventoryStatus, SkillObservation,
};

const ROUTING_DESCRIPTION_MAX_CHARS: usize = 240;

pub(super) fn group_logical_skills(
    observations: Vec<SkillObservation>,
) -> Vec<LogicalSkillInventoryItem> {
    let mut grouped: BTreeMap<String, Vec<SkillObservation>> = BTreeMap::new();
    for observation in observations {
        let scope = match observation.scope {
            crate::acp::types::AgentSkillScope::Global => "global",
            crate::acp::types::AgentSkillScope::Project => "project",
        };
        grouped
            .entry(format!("{scope}:{}", observation.skill_id))
            .or_default()
            .push(observation);
    }
    grouped
        .into_iter()
        .map(|(_, values)| build_logical(values))
        .collect()
}

fn build_logical(observations: Vec<SkillObservation>) -> LogicalSkillInventoryItem {
    let hashes = observations
        .iter()
        .filter_map(|value| value.content_tree_hash.as_ref())
        .collect::<BTreeSet<_>>();
    let conflict = hashes.len() > 1;
    let sources = observations
        .iter()
        .filter(|value| {
            value
                .locations
                .iter()
                .all(|location| location.projection_source.is_none())
        })
        .count();
    let duplicate = !conflict && hashes.len() == 1 && sources > 1;
    let agent_states = collect_agent_states(&observations);
    let read_only = observations.iter().all(|value| value.read_only);
    let unreadable = observations
        .iter()
        .all(|value| value.content_tree_hash.is_none());
    let stale_market_record = observations
        .iter()
        .any(|value| value.market_content_matches == Some(false));
    let status = inventory_status(
        conflict,
        duplicate,
        read_only,
        stale_market_record,
        unreadable,
        &agent_states,
    );
    let first = &observations[0];
    let dependencies = observations
        .iter()
        .flat_map(|value| value.dependencies.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let routing_description_chars = first
        .description
        .as_deref()
        .map(|value| value.chars().count())
        .unwrap_or_default();
    LogicalSkillInventoryItem {
        skill_id: first.skill_id.clone(),
        scope: first.scope,
        name: first.name.clone(),
        description: first.description.clone(),
        routing_description_chars,
        routing_description_over_limit: routing_description_chars > ROUTING_DESCRIPTION_MAX_CHARS,
        status,
        duplicate,
        conflict,
        local_only: observations
            .iter()
            .all(|value| value.market_skill_id.is_none()),
        plugin_available: true,
        stale_market_record,
        dependencies,
        observations,
        agent_states,
    }
}

fn collect_agent_states(observations: &[SkillObservation]) -> Vec<SkillAgentState> {
    let mut states: BTreeMap<AgentType, (bool, usize)> = BTreeMap::new();
    for location in observations.iter().flat_map(|value| &value.locations) {
        for agent_type in &location.agent_types {
            let state = states.entry(*agent_type).or_default();
            state.0 |= location.enabled;
            state.1 += 1;
        }
    }
    states
        .into_iter()
        .map(|(agent_type, (enabled, location_count))| SkillAgentState {
            agent_type,
            requested_enabled: None,
            effective_enabled: enabled,
            actual_enabled: enabled,
            required_by: Vec::new(),
            blocked_reasons: Vec::new(),
            location_count,
        })
        .collect()
}

fn inventory_status(
    conflict: bool,
    duplicate: bool,
    read_only: bool,
    stale_market_record: bool,
    unreadable: bool,
    states: &[SkillAgentState],
) -> SkillInventoryStatus {
    if conflict {
        return SkillInventoryStatus::Conflict;
    }
    if duplicate {
        return SkillInventoryStatus::Duplicate;
    }
    if unreadable {
        return SkillInventoryStatus::Unreadable;
    }
    if stale_market_record {
        return SkillInventoryStatus::StaleMarketRecord;
    }
    if read_only {
        return SkillInventoryStatus::AgentBuiltin;
    }
    if states.iter().any(|state| !state.blocked_reasons.is_empty()) {
        return SkillInventoryStatus::Blocked;
    }
    if states
        .iter()
        .any(|state| state.effective_enabled != state.actual_enabled)
    {
        return SkillInventoryStatus::OutOfSync;
    }
    let enabled = states.iter().filter(|state| state.actual_enabled).count();
    if enabled == 0 {
        SkillInventoryStatus::InstalledInactive
    } else if enabled == states.len() {
        SkillInventoryStatus::InstalledActive
    } else {
        SkillInventoryStatus::Partial
    }
}

pub(super) fn refresh_inventory_status(skill: &mut LogicalSkillInventoryItem) {
    skill.status = inventory_status(
        skill.conflict,
        skill.duplicate,
        skill.observations.iter().all(|value| value.read_only),
        skill.stale_market_record,
        skill
            .observations
            .iter()
            .all(|value| value.content_tree_hash.is_none()),
        &skill.agent_states,
    );
}
