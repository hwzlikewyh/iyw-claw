mod activation;
mod group;
mod operations;
mod resolver;
mod scan;
mod types;

use std::collections::BTreeMap;

use sea_orm::DatabaseConnection;
use sha2::{Digest, Sha256};

use crate::acp::error::AcpError;
use crate::commands::{experts, internet_tools, managed_skills, office_tools};
use crate::db::service::{app_metadata_service, skill_activation_policy_service};

pub use activation::skill_activation_set_core;
pub use operations::{skill_reconcile_core, skill_take_over_core};
pub use types::{
    LogicalSkillInventoryItem, SkillActivationApplyStatus, SkillActivationSetRequest,
    SkillActivationSetResult, SkillAgentDescriptionBudget, SkillAgentState,
    SkillInventoryOwnership, SkillInventorySnapshot, SkillInventoryStatus, SkillMutationResult,
    SkillObservation, SkillObservedLocation, SkillReconcileRequest, SkillTakeOverRequest,
};

const DESCRIPTION_BUDGET_SOFT_LIMIT_CHARS: usize = 6_000;
const LEGACY_POLICY_MIGRATION_KEY: &str = "managed_skills.skill_activation_policy_migrated.v1";

struct LegacyPolicySpec {
    policy_key: &'static str,
    overrides_key: &'static str,
    default_enabled: bool,
    skill_ids: fn() -> Vec<String>,
}

pub async fn skill_inventory_list_core(
    conn: &DatabaseConnection,
    workspace_path: Option<&str>,
) -> Result<SkillInventorySnapshot, AcpError> {
    ensure_legacy_policy_migrated(conn).await?;
    let observations = scan::scan_observations(workspace_path)?;
    let mut skills = group::group_logical_skills(observations);
    apply_activation_policies(conn, workspace_path, &mut skills).await?;
    resolver::apply_effective_states(conn, &mut skills).await?;
    for skill in &mut skills {
        group::refresh_inventory_status(skill);
    }
    let revision = inventory_revision(&skills);
    let description_budgets = description_budgets(&skills);
    Ok(SkillInventorySnapshot {
        revision,
        workspace_path: workspace_path.map(ToOwned::to_owned),
        skills,
        description_budgets,
    })
}

fn description_budgets(skills: &[LogicalSkillInventoryItem]) -> Vec<SkillAgentDescriptionBudget> {
    let mut budgets = std::collections::BTreeMap::new();
    for skill in skills {
        for state in &skill.agent_states {
            if !state.effective_enabled {
                continue;
            }
            let budget = budgets
                .entry(state.agent_type)
                .or_insert((0_usize, 0_usize));
            budget.0 += 1;
            budget.1 += skill.routing_description_chars;
        }
    }
    budgets
        .into_iter()
        .map(
            |(agent_type, (skill_count, used_chars))| SkillAgentDescriptionBudget {
                agent_type,
                skill_count,
                used_chars,
                soft_limit_chars: DESCRIPTION_BUDGET_SOFT_LIMIT_CHARS,
                over_soft_limit: used_chars > DESCRIPTION_BUDGET_SOFT_LIMIT_CHARS,
            },
        )
        .collect()
}

async fn ensure_legacy_policy_migrated(conn: &DatabaseConnection) -> Result<(), AcpError> {
    if app_metadata_service::get_value(conn, LEGACY_POLICY_MIGRATION_KEY)
        .await
        .map_err(|error| AcpError::protocol(error.to_string()))?
        .as_deref()
        == Some("true")
    {
        return Ok(());
    }
    let mut found_legacy_policy = false;
    for spec in legacy_policy_specs() {
        found_legacy_policy |= migrate_legacy_family(conn, &spec).await?;
    }
    if !found_legacy_policy {
        return Ok(());
    }
    app_metadata_service::upsert_value(conn, LEGACY_POLICY_MIGRATION_KEY, "true")
        .await
        .map_err(|error| AcpError::protocol(error.to_string()))?;
    Ok(())
}

fn legacy_policy_specs() -> Vec<LegacyPolicySpec> {
    vec![
        LegacyPolicySpec {
            policy_key: managed_skills::EXPERTS_POLICY_KEY,
            overrides_key: managed_skills::EXPERTS_OVERRIDES_KEY,
            default_enabled: true,
            skill_ids: experts::managed_expert_ids,
        },
        LegacyPolicySpec {
            policy_key: managed_skills::OFFICE_TOOLS_POLICY_KEY,
            overrides_key: managed_skills::OFFICE_TOOLS_OVERRIDES_KEY,
            default_enabled: true,
            skill_ids: office_tools::managed_office_skill_ids,
        },
        LegacyPolicySpec {
            policy_key: managed_skills::INTERNET_TOOLS_POLICY_KEY,
            overrides_key: managed_skills::INTERNET_TOOLS_OVERRIDES_KEY,
            default_enabled: true,
            skill_ids: internet_tools::managed_internet_skill_ids,
        },
        LegacyPolicySpec {
            policy_key: managed_skills::CODEX_NATIVE_POLICY_KEY,
            overrides_key: managed_skills::CODEX_NATIVE_OVERRIDES_KEY,
            default_enabled: true,
            skill_ids: experts::managed_codex_native_ids,
        },
        LegacyPolicySpec {
            policy_key: managed_skills::COMPUTER_USE_POLICY_KEY,
            overrides_key: managed_skills::COMPUTER_USE_OVERRIDES_KEY,
            default_enabled: false,
            skill_ids: experts::managed_computer_use_ids,
        },
    ]
}

async fn migrate_legacy_family(
    conn: &DatabaseConnection,
    spec: &LegacyPolicySpec,
) -> Result<bool, AcpError> {
    let default_raw = app_metadata_service::get_value(conn, spec.policy_key)
        .await
        .map_err(|error| AcpError::protocol(error.to_string()))?;
    let overrides = load_legacy_overrides(conn, spec.overrides_key).await?;
    if default_raw.is_none() && overrides.is_none() {
        return Ok(false);
    }
    let default_enabled = default_raw
        .as_deref()
        .map(str::parse)
        .transpose()
        .map_err(|error| AcpError::protocol(format!("invalid legacy Skill policy: {error}")))?
        .unwrap_or(spec.default_enabled);
    let overrides = overrides.unwrap_or_default();
    for skill_id in (spec.skill_ids)() {
        for agent_type in managed_skills::supported_skill_agent_types() {
            let requested_enabled = overrides.get(&skill_id).copied().unwrap_or(default_enabled);
            skill_activation_policy_service::upsert(
                conn,
                skill_activation_policy_service::SkillActivationPolicyInput {
                    skill_id: skill_id.clone(),
                    scope: crate::acp::types::AgentSkillScope::Global,
                    workspace_key: String::new(),
                    agent_type,
                    requested_enabled,
                    policy_source: "legacy_migration".to_string(),
                },
            )
            .await
            .map_err(|error| AcpError::protocol(error.to_string()))?;
        }
    }
    Ok(true)
}

async fn load_legacy_overrides(
    conn: &DatabaseConnection,
    key: &str,
) -> Result<Option<BTreeMap<String, bool>>, AcpError> {
    let raw = app_metadata_service::get_value(conn, key)
        .await
        .map_err(|error| AcpError::protocol(error.to_string()))?;
    raw.map(|value| {
        serde_json::from_str(&value)
            .map_err(|error| AcpError::protocol(format!("invalid legacy Skill overrides: {error}")))
    })
    .transpose()
}

async fn apply_activation_policies(
    conn: &DatabaseConnection,
    workspace_path: Option<&str>,
    skills: &mut [LogicalSkillInventoryItem],
) -> Result<(), AcpError> {
    let workspace_key = workspace_key(workspace_path);
    let mut policies = skill_activation_policy_service::list_for_workspace(conn, &workspace_key)
        .await
        .map_err(|error| AcpError::protocol(error.to_string()))?;
    let deepseek_key =
        workspace_key_for_agent(crate::models::agent::AgentType::DeepSeek, workspace_path);
    if deepseek_key != workspace_key {
        policies.extend(
            skill_activation_policy_service::list_for_workspace(conn, &deepseek_key)
                .await
                .map_err(|error| AcpError::protocol(error.to_string()))?,
        );
    }
    for policy in policies {
        let Ok(agent_type) = serde_json::from_str(&policy.agent_type) else {
            continue;
        };
        let Some(skill) = skills.iter_mut().find(|skill| {
            skill.skill_id == policy.skill_id
                && skill_activation_policy_service::scope_key(skill.scope) == policy.scope
        }) else {
            continue;
        };
        if let Some(state) = skill
            .agent_states
            .iter_mut()
            .find(|state| state.agent_type == agent_type)
        {
            state.requested_enabled = Some(policy.requested_enabled);
            state.effective_enabled = policy.requested_enabled;
        } else {
            skill.agent_states.push(SkillAgentState {
                agent_type,
                requested_enabled: Some(policy.requested_enabled),
                effective_enabled: policy.requested_enabled,
                actual_enabled: false,
                required_by: Vec::new(),
                blocked_reasons: Vec::new(),
                location_count: 0,
            });
        }
    }
    Ok(())
}

pub(crate) fn workspace_key(workspace_path: Option<&str>) -> String {
    let Some(path) = workspace_path.filter(|value| !value.trim().is_empty()) else {
        return String::new();
    };
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| std::path::PathBuf::from(path))
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase()
}

pub(crate) fn workspace_key_for_agent(
    agent_type: crate::models::agent::AgentType,
    workspace_path: Option<&str>,
) -> String {
    let Some(path) = workspace_path.filter(|value| !value.trim().is_empty()) else {
        return String::new();
    };
    let base = crate::commands::acp::skill_workspace_base(agent_type, path);
    workspace_key(Some(base.to_string_lossy().as_ref()))
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn skill_inventory_list(
    workspace_path: Option<String>,
    db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<SkillInventorySnapshot, AcpError> {
    skill_inventory_list_core(&db.conn, workspace_path.as_deref()).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn skill_activation_set(
    request: SkillActivationSetRequest,
    db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<SkillActivationSetResult, AcpError> {
    skill_activation_set_core(&db.conn, request).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn skill_take_over(
    request: SkillTakeOverRequest,
    db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<SkillMutationResult, AcpError> {
    skill_take_over_core(&db.conn, request).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn skill_reconcile(
    request: SkillReconcileRequest,
    db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<SkillMutationResult, AcpError> {
    skill_reconcile_core(&db.conn, request).await
}

fn inventory_revision(skills: &[LogicalSkillInventoryItem]) -> String {
    let mut hasher = Sha256::new();
    for skill in skills {
        hasher.update(skill.skill_id.as_bytes());
        hasher.update(format!("{:?}", skill.scope).as_bytes());
        hasher.update(format!("{:?}", skill.status).as_bytes());
        hasher.update([u8::from(skill.plugin_available)]);
        for observation in &skill.observations {
            hasher.update(observation.canonical_path.as_bytes());
            hasher.update(
                observation
                    .content_tree_hash
                    .as_deref()
                    .unwrap_or("unreadable")
                    .as_bytes(),
            );
        }
        for state in &skill.agent_states {
            hasher.update(format!("{:?}", state.agent_type).as_bytes());
            hasher.update([state.requested_enabled.map(u8::from).unwrap_or(2)]);
            hasher.update([u8::from(state.effective_enabled)]);
            hasher.update([u8::from(state.actual_enabled)]);
        }
    }
    format!("{:x}", hasher.finalize())
}
