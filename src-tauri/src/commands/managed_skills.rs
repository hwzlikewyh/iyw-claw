use std::collections::BTreeMap;
use std::sync::OnceLock;

use sea_orm::{DatabaseConnection, TransactionTrait};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::acp::registry;
use crate::app_error::AppCommandError;
use crate::commands::acp::skill_storage_spec;
use crate::commands::experts::{self, LinkOpResult};
use crate::commands::internet_tools;
use crate::commands::office_tools;
use crate::db::service::{agent_setting_service, app_metadata_service};
#[cfg(feature = "tauri-runtime")]
use crate::db::AppDatabase;
use crate::models::agent::AgentType;

pub const EXPERTS_POLICY_KEY: &str = "managed_skills.experts.enabled.v1";
pub const OFFICE_TOOLS_POLICY_KEY: &str = "managed_skills.office_tools.enabled.v1";
pub const INTERNET_TOOLS_POLICY_KEY: &str = "managed_skills.internet_tools.enabled.v1";
pub const EXPERTS_OVERRIDES_KEY: &str = "managed_skills.experts.overrides.v1";
pub const OFFICE_TOOLS_OVERRIDES_KEY: &str = "managed_skills.office_tools.overrides.v1";
pub const INTERNET_TOOLS_OVERRIDES_KEY: &str = "managed_skills.internet_tools.overrides.v1";

fn policy_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub(crate) fn supported_skill_agent_types() -> Vec<AgentType> {
    registry::all_acp_agents()
        .into_iter()
        .filter(|agent_type| skill_storage_spec(*agent_type).is_some())
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedSkillFamily {
    Experts,
    OfficeTools,
    InternetTools,
}

const MANAGED_SKILL_FAMILIES: [ManagedSkillFamily; 3] = [
    ManagedSkillFamily::Experts,
    ManagedSkillFamily::OfficeTools,
    ManagedSkillFamily::InternetTools,
];

impl ManagedSkillFamily {
    fn policy_key(self) -> &'static str {
        match self {
            Self::Experts => EXPERTS_POLICY_KEY,
            Self::OfficeTools => OFFICE_TOOLS_POLICY_KEY,
            Self::InternetTools => INTERNET_TOOLS_POLICY_KEY,
        }
    }

    fn overrides_key(self) -> &'static str {
        match self {
            Self::Experts => EXPERTS_OVERRIDES_KEY,
            Self::OfficeTools => OFFICE_TOOLS_OVERRIDES_KEY,
            Self::InternetTools => INTERNET_TOOLS_OVERRIDES_KEY,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedSkillGlobalState {
    pub experts_enabled: bool,
    pub office_tools_enabled: bool,
    pub internet_tools_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedSkillState {
    pub skill_id: String,
    pub enabled: bool,
    pub ready: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedSkillFamilyState {
    pub family: ManagedSkillFamily,
    pub all_enabled: bool,
    pub skills: Vec<ManagedSkillState>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedSkillSyncReport {
    pub family: ManagedSkillFamily,
    pub enabled: bool,
    pub skill_id: Option<String>,
    pub results: Vec<LinkOpResult>,
    pub touched_agents: Vec<AgentType>,
}

fn normalized_override(default_enabled: bool, enabled: bool) -> Option<bool> {
    (default_enabled != enabled).then_some(enabled)
}

fn family_skill_ids(family: ManagedSkillFamily) -> Vec<String> {
    match family {
        ManagedSkillFamily::Experts => experts::managed_expert_ids(),
        ManagedSkillFamily::OfficeTools => office_tools::managed_office_skill_ids(),
        ManagedSkillFamily::InternetTools => internet_tools::managed_internet_skill_ids(),
    }
}

fn family_ready_skill_ids(family: ManagedSkillFamily) -> Vec<String> {
    match family {
        ManagedSkillFamily::Experts => experts::managed_ready_expert_ids(),
        ManagedSkillFamily::OfficeTools => office_tools::managed_ready_office_skill_ids(),
        ManagedSkillFamily::InternetTools => internet_tools::managed_ready_internet_skill_ids(),
    }
}

fn family_knows_skill(family: ManagedSkillFamily, skill_id: &str) -> bool {
    family_skill_ids(family)
        .iter()
        .any(|known| known == skill_id)
}

fn is_enable_target(agent_type: AgentType, enabled: bool, env_json: Option<&str>) -> bool {
    if !enabled || skill_storage_spec(agent_type).is_none() {
        return false;
    }
    if agent_type != AgentType::Pi {
        return true;
    }
    let custom_dir = env_json
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
        .and_then(|value| {
            value
                .get("PI_CODING_AGENT_DIR")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        });
    custom_dir.is_none_or(|value| value.trim().is_empty())
}

async fn ensure_agent_settings(conn: &DatabaseConnection) -> Result<(), AppCommandError> {
    let defaults = registry::all_acp_agents()
        .into_iter()
        .enumerate()
        .map(
            |(index, agent_type)| agent_setting_service::AgentDefaultInput {
                agent_type,
                registry_id: registry::registry_id_for(agent_type).to_string(),
                default_sort_order: index as i32,
            },
        )
        .collect::<Vec<_>>();
    agent_setting_service::ensure_defaults(conn, &defaults)
        .await
        .map_err(AppCommandError::from)
}

async fn load_policy(
    conn: &DatabaseConnection,
    key: &str,
) -> Result<Option<bool>, AppCommandError> {
    let raw = app_metadata_service::get_value(conn, key)
        .await
        .map_err(AppCommandError::from)?;
    raw.map(|value| {
        value.parse::<bool>().map_err(|error| {
            AppCommandError::configuration_invalid(format!(
                "Invalid managed skill policy '{key}': {error}"
            ))
        })
    })
    .transpose()
}

async fn load_overrides_optional(
    conn: &DatabaseConnection,
    family: ManagedSkillFamily,
) -> Result<Option<BTreeMap<String, bool>>, AppCommandError> {
    let raw = app_metadata_service::get_value(conn, family.overrides_key())
        .await
        .map_err(AppCommandError::from)?;
    raw.map(|value| {
        serde_json::from_str(&value).map_err(|error| {
            AppCommandError::configuration_invalid(format!(
                "Invalid managed skill overrides '{}': {error}",
                family.overrides_key()
            ))
        })
    })
    .transpose()
}

async fn load_overrides(
    conn: &DatabaseConnection,
    family: ManagedSkillFamily,
) -> Result<BTreeMap<String, bool>, AppCommandError> {
    Ok(load_overrides_optional(conn, family)
        .await?
        .unwrap_or_default())
}

async fn persist_overrides(
    conn: &DatabaseConnection,
    family: ManagedSkillFamily,
    overrides: &BTreeMap<String, bool>,
) -> Result<(), AppCommandError> {
    let value = serde_json::to_string(overrides).map_err(|error| {
        AppCommandError::configuration_invalid(format!(
            "Failed to serialize managed skill overrides: {error}"
        ))
    })?;
    app_metadata_service::upsert_value(conn, family.overrides_key(), &value)
        .await
        .map_err(AppCommandError::from)
}

async fn persist_family_default(
    conn: &DatabaseConnection,
    family: ManagedSkillFamily,
    enabled: bool,
) -> Result<(), AppCommandError> {
    let transaction = conn
        .begin()
        .await
        .map_err(|error| AppCommandError::database_error(error.to_string()))?;
    app_metadata_service::upsert_value(&transaction, family.policy_key(), &enabled.to_string())
        .await
        .map_err(AppCommandError::from)?;
    app_metadata_service::upsert_value(&transaction, family.overrides_key(), "{}")
        .await
        .map_err(AppCommandError::from)?;
    transaction
        .commit()
        .await
        .map_err(|error| AppCommandError::database_error(error.to_string()))
}

async fn persist_skill_override(
    conn: &DatabaseConnection,
    family: ManagedSkillFamily,
    skill_id: &str,
    enabled: bool,
) -> Result<(), AppCommandError> {
    let default_enabled = load_policy(conn, family.policy_key())
        .await?
        .unwrap_or(false);
    let mut overrides = load_overrides(conn, family).await?;
    match normalized_override(default_enabled, enabled) {
        Some(value) => {
            overrides.insert(skill_id.to_string(), value);
        }
        None => {
            overrides.remove(skill_id);
        }
    }
    persist_overrides(conn, family, &overrides).await
}

async fn load_global_state(
    conn: &DatabaseConnection,
) -> Result<ManagedSkillGlobalState, AppCommandError> {
    Ok(ManagedSkillGlobalState {
        experts_enabled: load_policy(conn, EXPERTS_POLICY_KEY)
            .await?
            .unwrap_or(false),
        office_tools_enabled: load_policy(conn, OFFICE_TOOLS_POLICY_KEY)
            .await?
            .unwrap_or(false),
        internet_tools_enabled: load_policy(conn, INTERNET_TOOLS_POLICY_KEY)
            .await?
            .unwrap_or(false),
    })
}

async fn load_family_policy(
    conn: &DatabaseConnection,
    family: ManagedSkillFamily,
) -> Result<(bool, BTreeMap<String, bool>), AppCommandError> {
    let default_enabled = load_policy(conn, family.policy_key())
        .await?
        .unwrap_or(false);
    let overrides = load_overrides(conn, family).await?;
    Ok((default_enabled, overrides))
}

fn build_family_state(
    family: ManagedSkillFamily,
    default_enabled: bool,
    overrides: &BTreeMap<String, bool>,
) -> ManagedSkillFamilyState {
    let ready_ids = family_ready_skill_ids(family);
    let skills = family_skill_ids(family)
        .into_iter()
        .map(|skill_id| ManagedSkillState {
            enabled: overrides.get(&skill_id).copied().unwrap_or(default_enabled),
            ready: ready_ids.contains(&skill_id),
            skill_id,
        })
        .collect::<Vec<_>>();
    let all_enabled = !skills.is_empty() && skills.iter().all(|skill| skill.enabled);
    ManagedSkillFamilyState {
        family,
        all_enabled,
        skills,
    }
}

async fn load_family_state(
    conn: &DatabaseConnection,
    family: ManagedSkillFamily,
) -> Result<ManagedSkillFamilyState, AppCommandError> {
    let (default_enabled, overrides) = load_family_policy(conn, family).await?;
    Ok(build_family_state(family, default_enabled, &overrides))
}

fn migration_agent_types() -> Vec<AgentType> {
    supported_skill_agent_types()
}

async fn migrate_family_policy_with<F>(
    conn: &DatabaseConnection,
    family: ManagedSkillFamily,
    mut has_owned_link: F,
) -> Result<(), AppCommandError>
where
    F: FnMut(&str) -> bool,
{
    let default = load_policy(conn, family.policy_key()).await?;
    let current_overrides = load_overrides_optional(conn, family).await?;
    if default.is_some() && current_overrides.is_some() {
        return Ok(());
    }

    let mut overrides = current_overrides.unwrap_or_default();
    if default.is_none() {
        for skill_id in family_skill_ids(family) {
            if has_owned_link(&skill_id) {
                overrides.entry(skill_id).or_insert(true);
            }
        }
    }
    let value = serde_json::to_string(&overrides).map_err(|error| {
        AppCommandError::configuration_invalid(format!(
            "Failed to serialize managed skill overrides: {error}"
        ))
    })?;
    let transaction = conn
        .begin()
        .await
        .map_err(|error| AppCommandError::database_error(error.to_string()))?;
    if default.is_none() {
        app_metadata_service::upsert_value(&transaction, family.policy_key(), "false")
            .await
            .map_err(AppCommandError::from)?;
    }
    app_metadata_service::upsert_value(&transaction, family.overrides_key(), &value)
        .await
        .map_err(AppCommandError::from)?;
    transaction
        .commit()
        .await
        .map_err(|error| AppCommandError::database_error(error.to_string()))
}

async fn ensure_policies_migrated_locked(conn: &DatabaseConnection) -> Result<(), AppCommandError> {
    ensure_agent_settings(conn).await?;
    let agents = migration_agent_types();
    migrate_family_policy_with(conn, ManagedSkillFamily::Experts, |skill_id| {
        experts::managed_expert_has_owned_link(skill_id, &agents)
    })
    .await?;
    migrate_family_policy_with(conn, ManagedSkillFamily::OfficeTools, |skill_id| {
        office_tools::managed_office_skill_has_owned_link(skill_id, &agents)
    })
    .await?;
    migrate_family_policy_with(conn, ManagedSkillFamily::InternetTools, |skill_id| {
        internet_tools::managed_internet_skill_has_owned_link(skill_id, &agents)
    })
    .await?;
    Ok(())
}

pub async fn ensure_policies_migrated(conn: &DatabaseConnection) -> Result<(), AppCommandError> {
    let _guard = policy_lock().lock().await;
    ensure_policies_migrated_locked(conn).await
}

pub async fn get_global_state_core(
    conn: &DatabaseConnection,
) -> Result<ManagedSkillGlobalState, AppCommandError> {
    let _guard = policy_lock().lock().await;
    ensure_policies_migrated_locked(conn).await?;
    load_global_state(conn).await
}

pub async fn get_family_state_core(
    conn: &DatabaseConnection,
    family: ManagedSkillFamily,
) -> Result<ManagedSkillFamilyState, AppCommandError> {
    let _guard = policy_lock().lock().await;
    ensure_policies_migrated_locked(conn).await?;
    load_family_state(conn, family).await
}

fn expand_skill_targets(
    agents: &[(AgentType, bool)],
    skills: &[(String, bool)],
) -> Vec<(AgentType, String, bool)> {
    agents
        .iter()
        .filter(|(_, eligible)| *eligible)
        .flat_map(|(agent_type, _)| {
            skills
                .iter()
                .map(move |(skill_id, desired)| (*agent_type, skill_id.clone(), *desired))
        })
        .collect()
}

async fn agent_eligibility(
    conn: &DatabaseConnection,
) -> Result<Vec<(AgentType, bool)>, AppCommandError> {
    ensure_agent_settings(conn).await?;
    let settings = agent_setting_service::list_map_by_agent_type(conn)
        .await
        .map_err(AppCommandError::from)?;
    Ok(supported_skill_agent_types()
        .into_iter()
        .map(|agent_type| {
            let eligible = settings.get(&agent_type).is_some_and(|setting| {
                is_enable_target(agent_type, setting.enabled, setting.env_json.as_deref())
            });
            (agent_type, eligible)
        })
        .collect())
}

fn desired_skills(state: &ManagedSkillFamilyState) -> Vec<(String, bool)> {
    state
        .skills
        .iter()
        .map(|skill| (skill.skill_id.clone(), skill.enabled))
        .collect()
}

fn touched_agents(results: &[LinkOpResult]) -> Vec<AgentType> {
    let mut touched = Vec::new();
    for result in results.iter().filter(|result| result.ok) {
        if !touched.contains(&result.agent_type) {
            touched.push(result.agent_type);
        }
    }
    touched
}

async fn reconcile_targets(
    family: ManagedSkillFamily,
    enabled: bool,
    skill_id: Option<String>,
    targets: &[(AgentType, String, bool)],
) -> ManagedSkillSyncReport {
    let results = match family {
        ManagedSkillFamily::Experts => experts::reconcile_managed_experts(targets).await,
        ManagedSkillFamily::OfficeTools => {
            office_tools::reconcile_managed_office_tools(targets).await
        }
        ManagedSkillFamily::InternetTools => {
            internet_tools::reconcile_managed_internet_tools(targets).await
        }
    };
    let touched_agents = touched_agents(&results);
    ManagedSkillSyncReport {
        family,
        enabled,
        skill_id,
        results,
        touched_agents,
    }
}

pub async fn reconcile_family_core(
    conn: &DatabaseConnection,
    family: ManagedSkillFamily,
    enabled: bool,
) -> Result<ManagedSkillSyncReport, AppCommandError> {
    let agents = agent_eligibility(conn).await?;
    let skills = family_skill_ids(family)
        .into_iter()
        .map(|skill_id| (skill_id, enabled))
        .collect::<Vec<_>>();
    let targets = expand_skill_targets(&agents, &skills);
    Ok(reconcile_targets(family, enabled, None, &targets).await)
}

pub async fn reconcile_persisted_family_core(
    conn: &DatabaseConnection,
    family: ManagedSkillFamily,
) -> Result<ManagedSkillSyncReport, AppCommandError> {
    let _guard = policy_lock().lock().await;
    ensure_policies_migrated_locked(conn).await?;
    let state = load_family_state(conn, family).await?;
    let agents = agent_eligibility(conn).await?;
    let targets = expand_skill_targets(&agents, &desired_skills(&state));
    Ok(reconcile_targets(family, state.all_enabled, None, &targets).await)
}

pub async fn set_global_enabled_core(
    conn: &DatabaseConnection,
    family: ManagedSkillFamily,
    enabled: bool,
) -> Result<ManagedSkillSyncReport, AppCommandError> {
    let _guard = policy_lock().lock().await;
    ensure_policies_migrated_locked(conn).await?;
    persist_family_default(conn, family, enabled).await?;
    let agents = agent_eligibility(conn).await?;
    let skills = family_skill_ids(family)
        .into_iter()
        .map(|skill_id| (skill_id, enabled))
        .collect::<Vec<_>>();
    let targets = expand_skill_targets(&agents, &skills);
    Ok(reconcile_targets(family, enabled, None, &targets).await)
}

pub async fn set_skill_enabled_core(
    conn: &DatabaseConnection,
    family: ManagedSkillFamily,
    skill_id: String,
    enabled: bool,
) -> Result<ManagedSkillSyncReport, AppCommandError> {
    if !family_knows_skill(family, &skill_id) {
        return Err(AppCommandError::invalid_input(format!(
            "Unknown managed skill '{skill_id}' for {family:?}"
        )));
    }
    let _guard = policy_lock().lock().await;
    ensure_policies_migrated_locked(conn).await?;
    persist_skill_override(conn, family, &skill_id, enabled).await?;
    let agents = agent_eligibility(conn).await?;
    let targets = expand_skill_targets(&agents, &[(skill_id.clone(), enabled)]);
    Ok(reconcile_targets(family, enabled, Some(skill_id), &targets).await)
}

pub async fn reconcile_all_core(
    conn: &DatabaseConnection,
) -> Result<Vec<ManagedSkillSyncReport>, AppCommandError> {
    let _guard = policy_lock().lock().await;
    ensure_policies_migrated_locked(conn).await?;
    let agents = agent_eligibility(conn).await?;
    reconcile_families_for_agents(conn, &agents).await
}

async fn reconcile_families_for_agents(
    conn: &DatabaseConnection,
    agents: &[(AgentType, bool)],
) -> Result<Vec<ManagedSkillSyncReport>, AppCommandError> {
    let mut reports = Vec::with_capacity(MANAGED_SKILL_FAMILIES.len());
    for family in MANAGED_SKILL_FAMILIES {
        let state = load_family_state(conn, family).await?;
        let targets = expand_skill_targets(agents, &desired_skills(&state));
        reports.push(reconcile_targets(family, state.all_enabled, None, &targets).await);
    }
    Ok(reports)
}

pub async fn reconcile_agent_core(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    agent_enabled: bool,
) -> Result<Vec<ManagedSkillSyncReport>, AppCommandError> {
    let _guard = policy_lock().lock().await;
    ensure_policies_migrated_locked(conn).await?;
    let setting = agent_setting_service::get_by_agent_type(conn, agent_type)
        .await
        .map_err(AppCommandError::from)?;
    let eligible = setting.as_ref().is_some_and(|setting| {
        is_enable_target(agent_type, agent_enabled, setting.env_json.as_deref())
    });
    let supported = skill_storage_spec(agent_type).is_some();
    let agents = supported
        .then_some((agent_type, eligible))
        .into_iter()
        .collect::<Vec<_>>();
    reconcile_families_for_agents(conn, &agents).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn managed_skills_get_global_state(
    db: tauri::State<'_, AppDatabase>,
) -> Result<ManagedSkillGlobalState, AppCommandError> {
    get_global_state_core(&db.conn).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn managed_skills_set_global_enabled(
    family: ManagedSkillFamily,
    enabled: bool,
    db: tauri::State<'_, AppDatabase>,
) -> Result<ManagedSkillSyncReport, AppCommandError> {
    set_global_enabled_core(&db.conn, family, enabled).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn managed_skills_get_family_state(
    family: ManagedSkillFamily,
    db: tauri::State<'_, AppDatabase>,
) -> Result<ManagedSkillFamilyState, AppCommandError> {
    get_family_state_core(&db.conn, family).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn managed_skills_set_skill_enabled(
    family: ManagedSkillFamily,
    skill_id: String,
    enabled: bool,
    db: tauri::State<'_, AppDatabase>,
) -> Result<ManagedSkillSyncReport, AppCommandError> {
    set_skill_enabled_core(&db.conn, family, skill_id, enabled).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn managed_skills_reconcile_family(
    family: ManagedSkillFamily,
    db: tauri::State<'_, AppDatabase>,
) -> Result<ManagedSkillSyncReport, AppCommandError> {
    reconcile_persisted_family_core(&db.conn, family).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acp::registry;
    use crate::db::service::agent_setting_service;
    use crate::db::service::app_metadata_service;
    use crate::db::test_helpers::fresh_in_memory_db;
    use serde_json::json;

    #[test]
    fn managed_skill_wire_types_use_stable_case_conventions() {
        assert_eq!(
            serde_json::to_value(ManagedSkillFamily::OfficeTools).unwrap(),
            json!("office_tools")
        );
        assert_eq!(
            serde_json::to_value(ManagedSkillFamily::InternetTools).unwrap(),
            json!("internet_tools")
        );
        let state = ManagedSkillGlobalState {
            experts_enabled: true,
            office_tools_enabled: false,
            internet_tools_enabled: true,
        };
        assert_eq!(
            serde_json::to_value(state).unwrap(),
            json!({
                "expertsEnabled": true,
                "officeToolsEnabled": false,
                "internetToolsEnabled": true,
            })
        );

        let report = ManagedSkillSyncReport {
            family: ManagedSkillFamily::Experts,
            enabled: true,
            skill_id: Some("brainstorming".to_string()),
            results: Vec::new(),
            touched_agents: Vec::new(),
        };
        assert_eq!(
            serde_json::to_value(report).unwrap(),
            json!({
                "family": "experts",
                "enabled": true,
                "skillId": "brainstorming",
                "results": [],
                "touchedAgents": [],
            })
        );

        let family_state = ManagedSkillFamilyState {
            family: ManagedSkillFamily::OfficeTools,
            all_enabled: false,
            skills: vec![ManagedSkillState {
                skill_id: "officecli-docx".to_string(),
                enabled: true,
                ready: false,
            }],
        };
        assert_eq!(
            serde_json::to_value(family_state).unwrap(),
            json!({
                "family": "office_tools",
                "allEnabled": false,
                "skills": [{
                    "skillId": "officecli-docx",
                    "enabled": true,
                    "ready": false,
                }],
            })
        );
    }

    #[tokio::test]
    async fn migration_persists_each_linked_skill_as_true_override() {
        let db = fresh_in_memory_db().await;
        let linked_id = family_skill_ids(ManagedSkillFamily::Experts)[0].clone();

        migrate_family_policy_with(&db.conn, ManagedSkillFamily::Experts, |skill_id| {
            skill_id == linked_id
        })
        .await
        .unwrap();

        assert_eq!(
            load_policy(&db.conn, EXPERTS_POLICY_KEY).await.unwrap(),
            Some(false)
        );
        assert_eq!(
            load_overrides(&db.conn, ManagedSkillFamily::Experts)
                .await
                .unwrap(),
            BTreeMap::from([(linked_id, true)])
        );
    }

    #[tokio::test]
    async fn migration_preserves_existing_family_default_when_adding_overrides_key() {
        let db = fresh_in_memory_db().await;
        app_metadata_service::upsert_value(&db.conn, EXPERTS_POLICY_KEY, "true")
            .await
            .unwrap();

        migrate_family_policy_with(&db.conn, ManagedSkillFamily::Experts, |_| false)
            .await
            .unwrap();

        assert_eq!(
            load_policy(&db.conn, EXPERTS_POLICY_KEY).await.unwrap(),
            Some(true)
        );
        assert!(load_overrides(&db.conn, ManagedSkillFamily::Experts)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn migration_preserves_existing_overrides_when_adding_family_default() {
        let db = fresh_in_memory_db().await;
        let skill_ids = family_skill_ids(ManagedSkillFamily::Experts);
        let explicitly_disabled = skill_ids[0].clone();
        let linked = skill_ids[1].clone();
        persist_overrides(
            &db.conn,
            ManagedSkillFamily::Experts,
            &BTreeMap::from([(explicitly_disabled.clone(), false)]),
        )
        .await
        .unwrap();

        migrate_family_policy_with(&db.conn, ManagedSkillFamily::Experts, |skill_id| {
            skill_id == linked
        })
        .await
        .unwrap();

        assert_eq!(
            load_policy(&db.conn, EXPERTS_POLICY_KEY).await.unwrap(),
            Some(false)
        );
        assert_eq!(
            load_overrides(&db.conn, ManagedSkillFamily::Experts)
                .await
                .unwrap(),
            BTreeMap::from([(explicitly_disabled, false), (linked, true)])
        );
    }

    #[test]
    fn migration_considers_every_supported_agent_regardless_of_enabled_state() {
        assert_eq!(migration_agent_types(), supported_skill_agent_types());
    }

    #[test]
    fn enable_targets_require_enabled_supported_manageable_agents() {
        assert!(is_enable_target(AgentType::Codex, true, None));
        assert!(!is_enable_target(AgentType::Codex, false, None));
        assert!(is_enable_target(AgentType::Pi, true, Some("{}")));
        assert!(!is_enable_target(
            AgentType::Pi,
            true,
            Some(r#"{"PI_CODING_AGENT_DIR":"D:/custom-pi"}"#),
        ));
    }

    #[tokio::test]
    async fn global_state_reads_independent_persisted_family_keys() {
        let db = fresh_in_memory_db().await;
        app_metadata_service::upsert_value(&db.conn, EXPERTS_POLICY_KEY, "true")
            .await
            .unwrap();
        app_metadata_service::upsert_value(&db.conn, OFFICE_TOOLS_POLICY_KEY, "false")
            .await
            .unwrap();

        let state = get_global_state_core(&db.conn).await.unwrap();

        assert!(state.experts_enabled);
        assert!(!state.office_tools_enabled);
    }

    #[tokio::test]
    async fn missing_policies_migrate_to_false_without_enabled_agents() {
        let db = fresh_in_memory_db().await;
        let defaults = registry::all_acp_agents()
            .into_iter()
            .enumerate()
            .map(
                |(index, agent_type)| agent_setting_service::AgentDefaultInput {
                    agent_type,
                    registry_id: registry::registry_id_for(agent_type).to_string(),
                    default_sort_order: index as i32,
                },
            )
            .collect::<Vec<_>>();
        agent_setting_service::ensure_defaults(&db.conn, &defaults)
            .await
            .unwrap();
        let codex = agent_setting_service::get_by_agent_type(&db.conn, AgentType::Codex)
            .await
            .unwrap()
            .unwrap();
        agent_setting_service::update(
            &db.conn,
            AgentType::Codex,
            agent_setting_service::AgentSettingsUpdate {
                enabled: false,
                env_json: codex.env_json,
                model_provider_id: codex.model_provider_id,
            },
        )
        .await
        .unwrap();

        ensure_policies_migrated(&db.conn).await.unwrap();

        assert_eq!(
            app_metadata_service::get_value(&db.conn, EXPERTS_POLICY_KEY)
                .await
                .unwrap()
                .as_deref(),
            Some("false")
        );
        assert_eq!(
            app_metadata_service::get_value(&db.conn, OFFICE_TOOLS_POLICY_KEY)
                .await
                .unwrap()
                .as_deref(),
            Some("false")
        );
    }

    #[test]
    fn touched_agents_include_only_successful_changes_once() {
        let results = vec![
            test_result(AgentType::Codex, true),
            test_result(AgentType::Codex, true),
            test_result(AgentType::Gemini, false),
            test_result(AgentType::Cline, true),
        ];

        assert_eq!(
            touched_agents(&results),
            vec![AgentType::Codex, AgentType::Cline]
        );
    }

    #[test]
    fn skill_override_is_sparse_against_family_default() {
        assert_eq!(normalized_override(false, false), None);
        assert_eq!(normalized_override(false, true), Some(true));
        assert_eq!(normalized_override(true, false), Some(false));
        assert_eq!(normalized_override(true, true), None);
    }

    #[test]
    fn per_skill_targets_skip_ineligible_agents() {
        let agents = vec![(AgentType::Codex, true), (AgentType::Gemini, false)];
        let skills = vec![
            ("enabled-skill".to_string(), true),
            ("disabled-skill".to_string(), false),
        ];

        assert_eq!(
            expand_skill_targets(&agents, &skills),
            vec![
                (AgentType::Codex, "enabled-skill".to_string(), true),
                (AgentType::Codex, "disabled-skill".to_string(), false),
            ]
        );
    }

    #[tokio::test]
    async fn family_state_resolves_default_and_skill_overrides() {
        let db = fresh_in_memory_db().await;
        app_metadata_service::upsert_value(&db.conn, EXPERTS_POLICY_KEY, "false")
            .await
            .unwrap();
        app_metadata_service::upsert_value(&db.conn, OFFICE_TOOLS_POLICY_KEY, "false")
            .await
            .unwrap();
        let first_id = experts::managed_expert_ids()[0].clone();
        let overrides = BTreeMap::from([(first_id.clone(), true)]);
        app_metadata_service::upsert_value(
            &db.conn,
            EXPERTS_OVERRIDES_KEY,
            &serde_json::to_string(&overrides).unwrap(),
        )
        .await
        .unwrap();
        app_metadata_service::upsert_value(&db.conn, OFFICE_TOOLS_OVERRIDES_KEY, "{}")
            .await
            .unwrap();

        let state = get_family_state_core(&db.conn, ManagedSkillFamily::Experts)
            .await
            .unwrap();

        assert!(!state.all_enabled);
        assert!(
            state
                .skills
                .iter()
                .find(|skill| skill.skill_id == first_id)
                .unwrap()
                .enabled
        );
        assert!(state
            .skills
            .iter()
            .filter(|skill| skill.skill_id != first_id)
            .all(|skill| !skill.enabled));
    }

    #[tokio::test]
    async fn master_persistence_clears_skill_overrides() {
        let db = fresh_in_memory_db().await;
        app_metadata_service::upsert_value(
            &db.conn,
            EXPERTS_OVERRIDES_KEY,
            r#"{"brainstorming":false}"#,
        )
        .await
        .unwrap();

        persist_family_default(&db.conn, ManagedSkillFamily::Experts, true)
            .await
            .unwrap();

        assert_eq!(
            app_metadata_service::get_value(&db.conn, EXPERTS_POLICY_KEY)
                .await
                .unwrap()
                .as_deref(),
            Some("true")
        );
        assert_eq!(
            app_metadata_service::get_value(&db.conn, EXPERTS_OVERRIDES_KEY)
                .await
                .unwrap()
                .as_deref(),
            Some("{}")
        );
    }

    #[tokio::test]
    async fn unknown_skill_toggle_is_rejected_before_persistence() {
        let db = fresh_in_memory_db().await;

        let error = set_skill_enabled_core(
            &db.conn,
            ManagedSkillFamily::Experts,
            "not-a-managed-expert".to_string(),
            true,
        )
        .await
        .unwrap_err();

        assert!(error.message.contains("Unknown managed skill"));
    }

    fn test_result(agent_type: AgentType, ok: bool) -> LinkOpResult {
        LinkOpResult {
            expert_id: "test-skill".to_string(),
            agent_type,
            ok,
            status: None,
            error: (!ok).then(|| "failed".to_string()),
        }
    }
}
