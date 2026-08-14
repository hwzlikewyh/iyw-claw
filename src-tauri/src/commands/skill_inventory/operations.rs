use std::path::Path;

use sea_orm::DatabaseConnection;

use crate::acp::error::AcpError;
use crate::acp::types::AgentSkillScope;
use crate::db::service::skill_activation_policy_service::{self, SkillActivationPolicyInput};

use super::skill_inventory_list_core;
use super::types::{
    SkillActivationApplyStatus, SkillMutationResult, SkillReconcileRequest, SkillTakeOverRequest,
};

pub async fn skill_take_over_core(
    conn: &DatabaseConnection,
    request: SkillTakeOverRequest,
) -> Result<SkillMutationResult, AcpError> {
    let before = skill_inventory_list_core(conn, request.workspace_path.as_deref()).await?;
    validate_revision(request.expected_revision.as_deref(), &before.revision)?;
    let observation = find_take_over_source(&before, &request.skill_id, &request.source_path)?;
    let layout = observation.layout;
    save_take_over_policy(conn, &request).await?;
    let apply_error = apply_take_over(conn, &request, layout).await;
    mutation_result(conn, request.workspace_path.as_deref(), apply_error).await
}

async fn save_take_over_policy(
    conn: &DatabaseConnection,
    request: &SkillTakeOverRequest,
) -> Result<(), AcpError> {
    skill_activation_policy_service::upsert(
        conn,
        SkillActivationPolicyInput {
            skill_id: request.skill_id.clone(),
            scope: AgentSkillScope::Global,
            workspace_key: String::new(),
            agent_type: request.agent_type,
            requested_enabled: true,
            policy_source: "user".to_string(),
        },
    )
    .await
    .map_err(|error| AcpError::protocol(error.to_string()))?;
    super::activation::ensure_dependency_policy_defaults(
        conn,
        &super::types::SkillActivationSetRequest {
            skill_id: request.skill_id.clone(),
            scope: AgentSkillScope::Global,
            workspace_path: request.workspace_path.clone(),
            agent_type: request.agent_type,
            enabled: true,
            sync_mode: request.sync_mode,
            expected_revision: request.expected_revision.clone(),
        },
    )
    .await?;

    Ok(())
}

async fn apply_take_over(
    conn: &DatabaseConnection,
    request: &SkillTakeOverRequest,
    layout: crate::acp::types::AgentSkillLayout,
) -> Option<String> {
    let apply = crate::commands::acp::take_over_skill_source_for_agent_core(
        conn,
        request.agent_type,
        &request.skill_id,
        Path::new(&request.source_path),
        layout,
        request.sync_mode.unwrap_or_default(),
    )
    .await;
    match apply {
        Ok(_) => {
            crate::commands::acp::reconcile_shared_market_skills_for_agent(conn, request.agent_type)
                .await
                .err()
                .map(|error| error.to_string())
        }
        Err(error) => Some(error.to_string()),
    }
}

pub async fn skill_reconcile_core(
    conn: &DatabaseConnection,
    request: SkillReconcileRequest,
) -> Result<SkillMutationResult, AcpError> {
    let result = match request.agent_type {
        Some(agent_type) => {
            crate::commands::acp::reconcile_shared_market_skills_for_agent(conn, agent_type).await
        }
        None => crate::commands::acp::reconcile_shared_market_skills(conn).await,
    };
    mutation_result(
        conn,
        request.workspace_path.as_deref(),
        result.err().map(|error| error.to_string()),
    )
    .await
}

fn validate_revision(expected: Option<&str>, actual: &str) -> Result<(), AcpError> {
    if expected.is_some_and(|value| value != actual) {
        Err(AcpError::protocol(
            "Skill inventory changed; refresh before applying this action",
        ))
    } else {
        Ok(())
    }
}

fn find_take_over_source<'a>(
    snapshot: &'a super::types::SkillInventorySnapshot,
    skill_id: &str,
    source_path: &str,
) -> Result<&'a super::types::SkillObservation, AcpError> {
    let source_key = normalized_path(source_path);
    snapshot
        .skills
        .iter()
        .find(|skill| skill.scope == AgentSkillScope::Global && skill.skill_id == skill_id)
        .and_then(|skill| {
            skill.observations.iter().find(|observation| {
                normalized_path(&observation.canonical_path) == source_key
                    || observation
                        .locations
                        .iter()
                        .any(|location| normalized_path(&location.path) == source_key)
            })
        })
        .ok_or_else(|| AcpError::protocol("selected Skill source is no longer installed"))
}

async fn mutation_result(
    conn: &DatabaseConnection,
    workspace_path: Option<&str>,
    error: Option<String>,
) -> Result<SkillMutationResult, AcpError> {
    let snapshot = skill_inventory_list_core(conn, workspace_path).await?;
    Ok(SkillMutationResult {
        status: if error.is_none() {
            SkillActivationApplyStatus::InSync
        } else {
            SkillActivationApplyStatus::OutOfSync
        },
        error,
        snapshot,
    })
}

fn normalized_path(value: &str) -> String {
    std::fs::canonicalize(value)
        .unwrap_or_else(|_| value.into())
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .replace('\\', "/")
        .to_ascii_lowercase()
}
