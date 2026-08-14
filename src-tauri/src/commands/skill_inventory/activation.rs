use sea_orm::DatabaseConnection;
use std::collections::BTreeSet;
use std::path::Path;

use crate::acp::error::AcpError;
use crate::db::service::skill_activation_policy_service::{self, SkillActivationPolicyInput};

use super::types::{
    SkillActivationApplyStatus, SkillActivationSetRequest, SkillActivationSetResult,
    SkillInventorySnapshot,
};
use super::{skill_inventory_list_core, workspace_key};

pub async fn skill_activation_set_core(
    conn: &DatabaseConnection,
    request: SkillActivationSetRequest,
) -> Result<SkillActivationSetResult, AcpError> {
    if request.scope == crate::acp::types::AgentSkillScope::Project
        && workspace_key(request.workspace_path.as_deref()).is_empty()
    {
        return Err(AcpError::protocol(
            "project Skill activation requires a workspace path",
        ));
    }
    let before = skill_inventory_list_core(conn, request.workspace_path.as_deref()).await?;
    validate_request(&request, &before)?;
    let policy_workspace_key = match request.scope {
        crate::acp::types::AgentSkillScope::Global => String::new(),
        crate::acp::types::AgentSkillScope::Project => {
            workspace_key(request.workspace_path.as_deref())
        }
    };
    skill_activation_policy_service::upsert(
        conn,
        SkillActivationPolicyInput {
            skill_id: request.skill_id.clone(),
            scope: request.scope,
            workspace_key: policy_workspace_key,
            agent_type: request.agent_type,
            requested_enabled: request.enabled,
            policy_source: "user".to_string(),
        },
    )
    .await
    .map_err(|error| AcpError::protocol(error.to_string()))?;
    if request.scope == crate::acp::types::AgentSkillScope::Global {
        ensure_dependency_policy_defaults(conn, &request).await?;
    }

    let policy_snapshot =
        skill_inventory_list_core(conn, request.workspace_path.as_deref()).await?;
    let effective_target =
        activation_state(&request, &policy_snapshot).is_some_and(|value| value.effective_enabled);
    let desired_projection = request.enabled && effective_target;
    let mut apply_error = None;
    if request.scope == crate::acp::types::AgentSkillScope::Global {
        match crate::commands::acp::reconcile_shared_market_skills_for_agent(
            conn,
            request.agent_type,
        )
        .await
        {
            Ok(()) => apply_error = None,
            Err(error) => merge_error(&mut apply_error, error.to_string()),
        }
    } else if let Err(error) = crate::commands::acp::set_skill_projection_for_agent(
        request.agent_type,
        request.scope,
        &request.skill_id,
        request.workspace_path.as_deref(),
        desired_projection,
        request.sync_mode.unwrap_or_default(),
    ) {
        apply_error = Some(error.to_string());
    }
    let after = skill_inventory_list_core(conn, request.workspace_path.as_deref()).await?;
    let state = activation_state(&request, &after);
    let actual_enabled = state.is_some_and(|value| value.actual_enabled);
    let effective_enabled = state.is_some_and(|value| value.effective_enabled);
    let in_sync = apply_error.is_none() && actual_enabled == effective_enabled;
    tracing::info!(
        skill_id = %request.skill_id,
        agent_type = %request.agent_type,
        scope = ?request.scope,
        requested_enabled = request.enabled,
        actual_enabled,
        in_sync,
        error = apply_error.as_deref().unwrap_or(""),
        "[skill-inventory] activation reconciled"
    );
    Ok(SkillActivationSetResult {
        skill_id: request.skill_id,
        scope: request.scope,
        agent_type: request.agent_type,
        requested_enabled: request.enabled,
        effective_enabled,
        actual_enabled,
        status: if in_sync {
            SkillActivationApplyStatus::InSync
        } else {
            SkillActivationApplyStatus::OutOfSync
        },
        error: apply_error,
        revision: after.revision,
    })
}

pub(super) async fn ensure_dependency_policy_defaults(
    conn: &DatabaseConnection,
    request: &SkillActivationSetRequest,
) -> Result<(), AcpError> {
    let source = crate::commands::acp::shared_skills_dir().join(&request.skill_id);
    let Some(marker) = crate::commands::acp::read_market_skill_marker(&source) else {
        return Ok(());
    };
    let existing = crate::db::service::skill_activation_policy_service::list_global_for_agent(
        conn,
        request.agent_type,
    )
    .await
    .map_err(|error| AcpError::protocol(error.to_string()))?
    .into_iter()
    .map(|row| row.skill_id)
    .collect::<BTreeSet<_>>();
    let mut pending = marker
        .dependencies
        .into_iter()
        .map(|dependency| dependency.slug)
        .collect::<Vec<_>>();
    let mut visited = BTreeSet::new();
    while let Some(skill_id) = pending.pop() {
        if !visited.insert(skill_id.clone()) {
            continue;
        }
        if !existing.contains(&skill_id) {
            crate::db::service::skill_activation_policy_service::upsert(
                conn,
                crate::db::service::skill_activation_policy_service::SkillActivationPolicyInput {
                    skill_id: skill_id.clone(),
                    scope: crate::acp::types::AgentSkillScope::Global,
                    workspace_key: String::new(),
                    agent_type: request.agent_type,
                    requested_enabled: false,
                    policy_source: "install_default".to_string(),
                },
            )
            .await
            .map_err(|error| AcpError::protocol(error.to_string()))?;
        }
        let dependency_path = crate::commands::acp::shared_skills_dir().join(&skill_id);
        if let Some(next) =
            crate::commands::acp::read_market_skill_marker(Path::new(&dependency_path))
        {
            pending.extend(
                next.dependencies
                    .into_iter()
                    .map(|dependency| dependency.slug),
            );
        }
    }
    Ok(())
}

fn validate_request(
    request: &SkillActivationSetRequest,
    snapshot: &SkillInventorySnapshot,
) -> Result<(), AcpError> {
    if request
        .expected_revision
        .as_deref()
        .is_some_and(|revision| revision != snapshot.revision)
    {
        return Err(AcpError::protocol(
            "Skill inventory changed; refresh before applying this setting",
        ));
    }
    let skill = snapshot
        .skills
        .iter()
        .find(|skill| skill.skill_id == request.skill_id && skill.scope == request.scope)
        .ok_or_else(|| AcpError::protocol(format!("skill not found: {}", request.skill_id)))?;
    if skill.conflict || skill.duplicate {
        return Err(AcpError::protocol(
            "Resolve duplicate or conflicting Skill locations before changing activation",
        ));
    }
    if skill
        .observations
        .iter()
        .all(|observation| observation.read_only)
    {
        return Err(AcpError::protocol("built-in system skills are read-only"));
    }
    Ok(())
}

fn activation_state<'a>(
    request: &SkillActivationSetRequest,
    snapshot: &'a SkillInventorySnapshot,
) -> Option<&'a super::types::SkillAgentState> {
    snapshot
        .skills
        .iter()
        .find(|skill| skill.skill_id == request.skill_id && skill.scope == request.scope)
        .and_then(|skill| {
            skill
                .agent_states
                .iter()
                .find(|state| state.agent_type == request.agent_type)
        })
}

fn merge_error(current: &mut Option<String>, incoming: String) {
    match current {
        Some(value) => {
            value.push_str("; ");
            value.push_str(&incoming);
        }
        None => *current = Some(incoming),
    }
}
