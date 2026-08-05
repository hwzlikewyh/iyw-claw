use std::collections::BTreeMap;
use std::sync::OnceLock;

use sea_orm::{DatabaseConnection, TransactionTrait};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::acp::registry;
use crate::app_error::AppCommandError;
use crate::commands::acp::skill_storage_spec;
use crate::commands::computer_use;
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
pub const CODEX_NATIVE_POLICY_KEY: &str = "managed_skills.codex_native.enabled.v1";
pub const COMPUTER_USE_POLICY_KEY: &str = "managed_skills.computer_use.enabled.v1";
pub const EXPERTS_OVERRIDES_KEY: &str = "managed_skills.experts.overrides.v1";
pub const OFFICE_TOOLS_OVERRIDES_KEY: &str = "managed_skills.office_tools.overrides.v1";
pub const INTERNET_TOOLS_OVERRIDES_KEY: &str = "managed_skills.internet_tools.overrides.v1";
pub const CODEX_NATIVE_OVERRIDES_KEY: &str = "managed_skills.codex_native.overrides.v1";
pub const COMPUTER_USE_OVERRIDES_KEY: &str = "managed_skills.computer_use.overrides.v1";
const DEFAULT_ENABLED_MIGRATION_KEY: &str = "managed_skills.default_enabled.v2";

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
    CodexNative,
    ComputerUse,
}

const MANAGED_SKILL_FAMILIES: [ManagedSkillFamily; 5] = [
    ManagedSkillFamily::Experts,
    ManagedSkillFamily::OfficeTools,
    ManagedSkillFamily::InternetTools,
    ManagedSkillFamily::CodexNative,
    ManagedSkillFamily::ComputerUse,
];

impl ManagedSkillFamily {
    fn policy_key(self) -> &'static str {
        match self {
            Self::Experts => EXPERTS_POLICY_KEY,
            Self::OfficeTools => OFFICE_TOOLS_POLICY_KEY,
            Self::InternetTools => INTERNET_TOOLS_POLICY_KEY,
            Self::CodexNative => CODEX_NATIVE_POLICY_KEY,
            Self::ComputerUse => COMPUTER_USE_POLICY_KEY,
        }
    }

    fn overrides_key(self) -> &'static str {
        match self {
            Self::Experts => EXPERTS_OVERRIDES_KEY,
            Self::OfficeTools => OFFICE_TOOLS_OVERRIDES_KEY,
            Self::InternetTools => INTERNET_TOOLS_OVERRIDES_KEY,
            Self::CodexNative => CODEX_NATIVE_OVERRIDES_KEY,
            Self::ComputerUse => COMPUTER_USE_OVERRIDES_KEY,
        }
    }
}

fn family_default_enabled(family: ManagedSkillFamily) -> bool {
    !matches!(family, ManagedSkillFamily::ComputerUse)
}

fn family_is_user_configurable(family: ManagedSkillFamily) -> bool {
    matches!(
        family,
        ManagedSkillFamily::OfficeTools
            | ManagedSkillFamily::InternetTools
            | ManagedSkillFamily::ComputerUse
    )
}

fn family_allows_agent(_family: ManagedSkillFamily, _agent_type: AgentType) -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedSkillGlobalState {
    pub experts_enabled: bool,
    pub office_tools_enabled: bool,
    pub internet_tools_enabled: bool,
    pub codex_native_enabled: bool,
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
        ManagedSkillFamily::CodexNative => experts::managed_codex_native_ids(),
        ManagedSkillFamily::ComputerUse => experts::managed_computer_use_ids(),
    }
}

fn family_ready_skill_ids(family: ManagedSkillFamily) -> Vec<String> {
    match family {
        ManagedSkillFamily::Experts => experts::managed_ready_expert_ids(),
        ManagedSkillFamily::OfficeTools => office_tools::managed_ready_office_skill_ids(),
        ManagedSkillFamily::InternetTools => internet_tools::managed_ready_internet_skill_ids(),
        ManagedSkillFamily::CodexNative => experts::managed_ready_codex_native_ids(),
        ManagedSkillFamily::ComputerUse => experts::managed_ready_computer_use_ids(),
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
        .unwrap_or(family_default_enabled(family));
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
        experts_enabled: true,
        office_tools_enabled: load_policy(conn, OFFICE_TOOLS_POLICY_KEY)
            .await?
            .unwrap_or(family_default_enabled(ManagedSkillFamily::OfficeTools)),
        internet_tools_enabled: load_policy(conn, INTERNET_TOOLS_POLICY_KEY)
            .await?
            .unwrap_or(family_default_enabled(ManagedSkillFamily::InternetTools)),
        codex_native_enabled: true,
    })
}

async fn load_family_policy(
    conn: &DatabaseConnection,
    family: ManagedSkillFamily,
) -> Result<(bool, BTreeMap<String, bool>), AppCommandError> {
    if !family_is_user_configurable(family) {
        return Ok((true, BTreeMap::new()));
    }
    let default_enabled = load_policy(conn, family.policy_key())
        .await?
        .unwrap_or(family_default_enabled(family));
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
        app_metadata_service::upsert_value(
            &transaction,
            family.policy_key(),
            &family_default_enabled(family).to_string(),
        )
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

async fn migrate_default_enabled_v2(conn: &DatabaseConnection) -> Result<(), AppCommandError> {
    if app_metadata_service::get_value(conn, DEFAULT_ENABLED_MIGRATION_KEY)
        .await
        .map_err(AppCommandError::from)?
        .as_deref()
        == Some("true")
    {
        return Ok(());
    }

    let families = [
        ManagedSkillFamily::Experts,
        ManagedSkillFamily::OfficeTools,
        ManagedSkillFamily::InternetTools,
        ManagedSkillFamily::CodexNative,
    ];
    let mut migrated = Vec::with_capacity(families.len());
    for family in families {
        let mut overrides = load_overrides(conn, family).await?;
        if family_is_user_configurable(family) {
            overrides.retain(|_, enabled| !*enabled);
        } else {
            overrides.clear();
        }
        migrated.push((
            family,
            serde_json::to_string(&overrides)
                .map_err(|error| AppCommandError::configuration_invalid(error.to_string()))?,
        ));
    }

    let transaction = conn
        .begin()
        .await
        .map_err(|error| AppCommandError::database_error(error.to_string()))?;
    for (family, overrides) in migrated {
        app_metadata_service::upsert_value(&transaction, family.policy_key(), "true")
            .await
            .map_err(AppCommandError::from)?;
        app_metadata_service::upsert_value(&transaction, family.overrides_key(), &overrides)
            .await
            .map_err(AppCommandError::from)?;
    }
    app_metadata_service::upsert_value(&transaction, DEFAULT_ENABLED_MIGRATION_KEY, "true")
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
    migrate_family_policy_with(conn, ManagedSkillFamily::CodexNative, |skill_id| {
        experts::managed_expert_has_owned_link(skill_id, &agents)
    })
    .await?;
    migrate_family_policy_with(conn, ManagedSkillFamily::ComputerUse, |skill_id| {
        experts::managed_expert_has_owned_link(skill_id, &agents)
    })
    .await?;
    migrate_default_enabled_v2(conn).await?;
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
    family: ManagedSkillFamily,
    agents: &[(AgentType, bool)],
    skills: &[(String, bool)],
) -> Vec<(AgentType, String, bool)> {
    agents
        .iter()
        .filter(|(agent_type, _)| family_allows_agent(family, *agent_type))
        .flat_map(|(agent_type, eligible)| {
            skills.iter().map(move |(skill_id, desired)| {
                (*agent_type, skill_id.clone(), *eligible && *desired)
            })
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
                setting.installed_version.is_some()
                    && is_enable_target(agent_type, setting.enabled, setting.env_json.as_deref())
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

fn failed_link_detail(report: &ManagedSkillSyncReport) -> Option<String> {
    let failures = report
        .results
        .iter()
        .filter(|result| !result.ok)
        .map(|result| {
            format!(
                "{:?}: {}",
                result.agent_type,
                result
                    .error
                    .as_deref()
                    .unwrap_or("skill publication failed")
            )
        })
        .collect::<Vec<_>>();
    (!failures.is_empty()).then(|| failures.join("\n"))
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
        ManagedSkillFamily::CodexNative => {
            let results = experts::reconcile_managed_experts(targets).await;
            // With replacements published at the normal skills level, clear
            // the CLI's own bundled copies so sessions never see duplicate
            // entries (and dropped skills like openai-docs stay gone). Only
            // purge while at least one replacement is desired — a fully
            // disabled family leaves Codex's stock behavior untouched.
            if targets.iter().any(|(_, _, desired)| *desired) {
                purge_codex_system_skill_entries();
            }
            results
        }
        ManagedSkillFamily::ComputerUse => experts::reconcile_managed_experts(targets).await,
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

/// Codex CLI's version marker inside `~/.codex/skills/.system/`. Preserved
/// on purge: Codex only re-extracts its bundled skills when this marker no
/// longer matches the CLI's embedded bundle version, so keeping it prevents
/// a purge/re-extract loop. After a Codex upgrade rewrites the directory,
/// the next reconcile clears it again.
const CODEX_SYSTEM_MARKER_FILE: &str = ".codex-system-skills.marker";

/// Clear the bundled skill copies under `~/.codex/skills/.system/` (keeping
/// the version marker, see above). Entries held open by a running Codex
/// process fail to delete on Windows — that's tolerated, the next reconcile
/// retries.
fn purge_codex_system_skill_entries() {
    let system_dir = crate::commands::acp::codex_home_dir()
        .join("skills")
        .join(".system");
    purge_codex_system_entries_in(&system_dir);
}

fn purge_codex_system_entries_in(system_dir: &std::path::Path) {
    let entries = match std::fs::read_dir(system_dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        if entry.file_name().to_str() == Some(CODEX_SYSTEM_MARKER_FILE) {
            continue;
        }
        let path = entry.path();
        if let Err(error) = crate::commands::acp::remove_skill_entry(&path) {
            tracing::warn!(
                "[ManagedSkills] failed to clear Codex system skill entry {}: {error}",
                path.display()
            );
        }
    }
}

pub async fn reconcile_family_core(
    conn: &DatabaseConnection,
    family: ManagedSkillFamily,
    enabled: bool,
) -> Result<ManagedSkillSyncReport, AppCommandError> {
    let enabled = if family_is_user_configurable(family) {
        enabled
    } else {
        true
    };
    let agents = agent_eligibility(conn).await?;
    let skills = family_skill_ids(family)
        .into_iter()
        .map(|skill_id| (skill_id, enabled))
        .collect::<Vec<_>>();
    let targets = expand_skill_targets(family, &agents, &skills);
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
    let targets = expand_skill_targets(family, &agents, &desired_skills(&state));
    Ok(reconcile_targets(family, state.all_enabled, None, &targets).await)
}

pub async fn set_global_enabled_core(
    conn: &DatabaseConnection,
    family: ManagedSkillFamily,
    enabled: bool,
) -> Result<ManagedSkillSyncReport, AppCommandError> {
    if !family_is_user_configurable(family) {
        return Err(AppCommandError::invalid_input(format!(
            "The {family:?} skill family is managed by iyw-claw and cannot be disabled"
        )));
    }
    let _guard = policy_lock().lock().await;
    ensure_policies_migrated_locked(conn).await?;
    if family == ManagedSkillFamily::ComputerUse {
        return set_computer_use_enabled_locked(conn, None, enabled).await;
    }
    persist_family_default(conn, family, enabled).await?;
    let agents = agent_eligibility(conn).await?;
    let skills = family_skill_ids(family)
        .into_iter()
        .map(|skill_id| (skill_id, enabled))
        .collect::<Vec<_>>();
    let targets = expand_skill_targets(family, &agents, &skills);
    Ok(reconcile_targets(family, enabled, None, &targets).await)
}

pub async fn set_skill_enabled_core(
    conn: &DatabaseConnection,
    family: ManagedSkillFamily,
    skill_id: String,
    enabled: bool,
) -> Result<ManagedSkillSyncReport, AppCommandError> {
    if !family_is_user_configurable(family) {
        return Err(AppCommandError::invalid_input(format!(
            "Skills in the {family:?} family are managed by iyw-claw"
        )));
    }
    if !family_knows_skill(family, &skill_id) {
        return Err(AppCommandError::invalid_input(format!(
            "Unknown managed skill '{skill_id}' for {family:?}"
        )));
    }
    let _guard = policy_lock().lock().await;
    ensure_policies_migrated_locked(conn).await?;
    if family == ManagedSkillFamily::ComputerUse {
        return set_computer_use_enabled_locked(conn, Some(skill_id), enabled).await;
    }
    persist_skill_override(conn, family, &skill_id, enabled).await?;
    let agents = agent_eligibility(conn).await?;
    let targets = expand_skill_targets(family, &agents, &[(skill_id.clone(), enabled)]);
    Ok(reconcile_targets(family, enabled, Some(skill_id), &targets).await)
}

async fn persist_computer_use_policy(
    conn: &DatabaseConnection,
    skill_id: Option<&str>,
    enabled: bool,
) -> Result<(), AppCommandError> {
    match skill_id {
        Some(skill_id) => {
            persist_skill_override(conn, ManagedSkillFamily::ComputerUse, skill_id, enabled).await
        }
        None => persist_family_default(conn, ManagedSkillFamily::ComputerUse, enabled).await,
    }
}

fn computer_use_skill_states(skill_id: Option<&str>, enabled: bool) -> Vec<(String, bool)> {
    match skill_id {
        Some(skill_id) => vec![(skill_id.to_string(), enabled)],
        None => family_skill_ids(ManagedSkillFamily::ComputerUse)
            .into_iter()
            .map(|id| (id, enabled))
            .collect(),
    }
}

async fn restore_computer_use_state(
    conn: &DatabaseConnection,
    skill_id: Option<&str>,
    previous_enabled: bool,
    agents: &[(AgentType, bool)],
) {
    if let Err(error) = persist_computer_use_policy(conn, skill_id, previous_enabled).await {
        tracing::error!(error = %error.message, "[computer-use] policy rollback failed");
    }
    let skills = computer_use_skill_states(skill_id, previous_enabled);
    let targets = expand_skill_targets(ManagedSkillFamily::ComputerUse, agents, &skills);
    let report = reconcile_targets(
        ManagedSkillFamily::ComputerUse,
        previous_enabled,
        skill_id.map(str::to_string),
        &targets,
    )
    .await;
    if let Some(detail) = failed_link_detail(&report) {
        tracing::error!(detail, "[computer-use] skill publication rollback failed");
    }
    if let Err(error) = computer_use::set_enabled_core(conn, previous_enabled).await {
        tracing::error!(error = %error.message, "[computer-use] MCP rollback failed");
    }
}

async fn set_computer_use_enabled_locked(
    conn: &DatabaseConnection,
    skill_id: Option<String>,
    enabled: bool,
) -> Result<ManagedSkillSyncReport, AppCommandError> {
    let state = load_family_state(conn, ManagedSkillFamily::ComputerUse).await?;
    let previous_enabled = skill_id
        .as_ref()
        .and_then(|id| state.skills.iter().find(|skill| &skill.skill_id == id))
        .map(|skill| skill.enabled)
        .unwrap_or(state.all_enabled);
    let agents = agent_eligibility(conn).await?;
    if !enabled {
        computer_use::set_enabled_core(conn, false).await?;
    }
    if let Err(error) = persist_computer_use_policy(conn, skill_id.as_deref(), enabled).await {
        if !enabled && previous_enabled {
            let _ = computer_use::set_enabled_core(conn, true).await;
        }
        return Err(error);
    }
    if enabled {
        if let Err(error) = computer_use::set_enabled_core(conn, true).await {
            restore_computer_use_state(conn, skill_id.as_deref(), previous_enabled, &agents).await;
            return Err(error);
        }
    }
    let skills = computer_use_skill_states(skill_id.as_deref(), enabled);
    let targets = expand_skill_targets(ManagedSkillFamily::ComputerUse, &agents, &skills);
    let report = reconcile_targets(
        ManagedSkillFamily::ComputerUse,
        enabled,
        skill_id.clone(),
        &targets,
    )
    .await;
    if let Some(detail) = failed_link_detail(&report) {
        restore_computer_use_state(conn, skill_id.as_deref(), previous_enabled, &agents).await;
        return Err(AppCommandError::task_execution_failed(
            "Open Computer Use skill publication failed",
        )
        .with_detail(detail));
    }
    Ok(report)
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
        let targets = expand_skill_targets(family, agents, &desired_skills(&state));
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
        setting.installed_version.is_some()
            && is_enable_target(agent_type, agent_enabled, setting.env_json.as_deref())
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
