use chrono::{DateTime, Utc};
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};

use crate::acp::capability_policy::{
    refresh_once, Capability, CapabilityPolicyStore, PolicySnapshotView,
};
use crate::acp::version_center::CapabilityPolicyHttpFetcher;
use crate::app_error::AppCommandError;
use crate::db::entities::capability_preference;
use crate::db::service::capability_preference_service;

#[path = "capability_policy_decision.rs"]
mod decision;
pub use decision::{decision_core, require_existing_agent_capability_core};

const CLIENT_SUBJECT_ID: &str = "global";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilitySubjectKind {
    Agent,
    Client,
}

impl CapabilitySubjectKind {
    fn key(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Client => "client",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilitySubjectRequest {
    pub subject_kind: CapabilitySubjectKind,
    pub subject_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityPreferenceUpdateRequest {
    pub subject_kind: CapabilitySubjectKind,
    pub subject_id: String,
    pub capability: Capability,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityDecisionRequest {
    pub subject_kind: CapabilitySubjectKind,
    pub subject_id: String,
    pub capability: Capability,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityPreferenceView {
    pub subject_kind: String,
    pub subject_id: String,
    pub capability: String,
    pub enabled: bool,
    pub updated_at: DateTime<Utc>,
}

pub async fn snapshot_core(store: &CapabilityPolicyStore) -> PolicySnapshotView {
    store.view().await
}

pub async fn refresh_core(
    conn: &DatabaseConnection,
    store: &CapabilityPolicyStore,
) -> Result<PolicySnapshotView, AppCommandError> {
    let fetcher = CapabilityPolicyHttpFetcher::new(conn.clone());
    refresh_once(store, &fetcher).await.map_err(policy_error)
}

pub async fn list_preferences_core(
    conn: &DatabaseConnection,
    request: CapabilitySubjectRequest,
) -> Result<Vec<CapabilityPreferenceView>, AppCommandError> {
    validate_subject(request.subject_kind, &request.subject_id)?;
    let mut rows = capability_preference_service::list_for_subject(
        conn,
        request.subject_kind.key(),
        &request.subject_id,
    )
    .await
    .map_err(AppCommandError::from)?;
    rows.sort_by(|left, right| left.capability.cmp(&right.capability));
    Ok(rows
        .into_iter()
        .map(CapabilityPreferenceView::from)
        .collect())
}

pub async fn set_preference_core(
    conn: &DatabaseConnection,
    request: CapabilityPreferenceUpdateRequest,
) -> Result<CapabilityPreferenceView, AppCommandError> {
    validate_request_scope(
        request.subject_kind,
        &request.subject_id,
        request.capability,
    )?;
    let row = capability_preference_service::upsert(
        conn,
        capability_preference_service::CapabilityPreferenceInput {
            subject_kind: request.subject_kind.key().to_string(),
            subject_id: request.subject_id,
            capability: request.capability.key().to_string(),
            enabled: request.enabled,
        },
    )
    .await
    .map_err(AppCommandError::from)?;
    crate::acp::capability_policy::notify_runtime_policy_change();
    Ok(row.into())
}

fn validate_request_scope(
    kind: CapabilitySubjectKind,
    subject_id: &str,
    capability: Capability,
) -> Result<(), AppCommandError> {
    validate_subject(kind, subject_id)?;
    if (kind == CapabilitySubjectKind::Agent) != capability.is_agent_scoped() {
        return Err(AppCommandError::invalid_input(
            "Capability does not match its subject kind",
        ));
    }
    Ok(())
}

fn validate_subject(kind: CapabilitySubjectKind, subject_id: &str) -> Result<(), AppCommandError> {
    match kind {
        CapabilitySubjectKind::Client if subject_id == CLIENT_SUBJECT_ID => Ok(()),
        CapabilitySubjectKind::Agent if valid_platform_id(subject_id) => Ok(()),
        _ => Err(AppCommandError::invalid_input("Invalid capability subject")),
    }
}

fn valid_platform_id(value: &str) -> bool {
    !value.starts_with('0')
        && value.len() <= 19
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && value.parse::<i64>().is_ok_and(|id| id > 0)
}

fn policy_error(error: crate::acp::capability_policy::CapabilityPolicyError) -> AppCommandError {
    match error {
        crate::acp::capability_policy::CapabilityPolicyError::Transport(detail) => {
            AppCommandError::network("Capability policy refresh failed").with_detail(detail)
        }
        other => AppCommandError::configuration_invalid("Capability policy was rejected")
            .with_detail(other.to_string()),
    }
}

impl From<capability_preference::Model> for CapabilityPreferenceView {
    fn from(value: capability_preference::Model) -> Self {
        Self {
            subject_kind: value.subject_kind,
            subject_id: value.subject_id,
            capability: value.capability,
            enabled: value.enabled,
            updated_at: value.updated_at,
        }
    }
}
