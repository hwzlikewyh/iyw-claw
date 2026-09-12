use std::collections::BTreeMap;

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

use crate::app_error::AppCommandError;

use super::candidate_store;
use super::helpers::{conflict, contains_potential_secret};
use super::index_types::IndexItem;
use super::{UserMemoryLearningState, UserMemoryRecallScope, UserMemoryService};

const MAX_RETENTION_RECORDS: usize = 4_096;
const MAX_RETIRE_REASON_CHARS: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryRetention {
    pub content_digest: String,
    pub source_revision: String,
    pub expires_at: String,
    pub reason: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RetireMemoryRequest {
    pub memory_id: String,
    pub expected_revision: String,
    pub reason: String,
    pub expires_at: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetireMemoryResult {
    pub memory_id: String,
    pub expires_at: String,
    pub excluded_from_recall: bool,
    pub revision: String,
}

impl UserMemoryService {
    pub async fn retire_memory(
        &self,
        request: RetireMemoryRequest,
        scope: UserMemoryRecallScope,
    ) -> Result<RetireMemoryResult, AppCommandError> {
        let (_guard, _file_guard) = self.acquire_locks().await?;
        let now = Utc::now();
        let retention = prepare_retention(&request, now)?;
        self.recover_pending_transaction().await?;
        let policy = self.load_policy_unrecovered().await?;
        if !policy.enabled || !policy.agent_write_enabled {
            return Err(AppCommandError::permission_denied(
                "Memory maintenance is disabled",
            ));
        }
        let root = self.resolved_root()?;
        let mut state = candidate_store::read_state(root)?;
        let settings = super::index_source::readonly_snapshot(self, &policy)?;
        let snapshot = super::index_parse::build_index_snapshot(&settings, Some(&state));
        let item = find_target(&snapshot.items, &request, &scope)?;
        let retention = earliest_retention(&state, item, retention);
        state
            .retention
            .insert(request.memory_id.clone(), retention.clone());
        candidate_store::write_state(root, &state)?;
        self.schedule_index_refresh();
        let excluded = parse_time(&retention.expires_at)? <= now;
        tracing::info!(
            excluded_from_recall = excluded,
            memory_kind = %item.kind,
            "[memory-retention] memory validity updated"
        );
        Ok(RetireMemoryResult {
            memory_id: request.memory_id,
            expires_at: retention.expires_at,
            excluded_from_recall: excluded,
            revision: candidate_store::revision(&state)?,
        })
    }
}

fn prepare_retention(
    request: &RetireMemoryRequest,
    now: DateTime<Utc>,
) -> Result<MemoryRetention, AppCommandError> {
    let reason = request.reason.trim();
    if reason.is_empty()
        || reason.chars().count() > MAX_RETIRE_REASON_CHARS
        || contains_potential_secret(reason)
    {
        return Err(AppCommandError::invalid_input(
            "A concise, non-sensitive retirement reason is required",
        ));
    }
    let expiry = request
        .expires_at
        .as_deref()
        .map(parse_time)
        .transpose()?
        .unwrap_or(now);
    if !(1..=9999).contains(&chrono::Datelike::year(&expiry)) {
        return Err(AppCommandError::invalid_input(
            "Memory expiry year is outside the supported range",
        ));
    }
    Ok(MemoryRetention {
        content_digest: String::new(),
        source_revision: request.expected_revision.clone(),
        expires_at: expiry.to_rfc3339_opts(SecondsFormat::Millis, true),
        reason: reason.to_string(),
        updated_at: now.to_rfc3339_opts(SecondsFormat::Millis, true),
    })
}

fn validate_target(item: &IndexItem, request: &RetireMemoryRequest) -> Result<(), AppCommandError> {
    if item.source_revision != request.expected_revision {
        return Err(conflict(
            "Memory changed; recall the current item before retiring it",
        ));
    }
    if item.sensitive || !matches!(item.kind.as_str(), "memory" | "experience") {
        return Err(AppCommandError::invalid_input(
            "Only recalled user-memory entries and Agent experience can be retired",
        ));
    }
    Ok(())
}

fn find_target<'a>(
    items: &'a [IndexItem],
    request: &RetireMemoryRequest,
    scope: &UserMemoryRecallScope,
) -> Result<&'a IndexItem, AppCommandError> {
    let item = items
        .iter()
        .find(|item| {
            item.id == request.memory_id && scope.permits(&item.scope_type, &item.scope_key)
        })
        .ok_or_else(|| AppCommandError::not_found("Memory was not found in this scope"))?;
    validate_target(item, request)?;
    Ok(item)
}

fn earliest_retention(
    state: &UserMemoryLearningState,
    item: &IndexItem,
    proposed: MemoryRetention,
) -> MemoryRetention {
    if let Some(existing) = state.retention.get(&item.id).filter(|record| {
        record.content_digest == item.content_digest && record.expires_at <= proposed.expires_at
    }) {
        return existing.clone();
    }
    MemoryRetention {
        content_digest: item.content_digest.clone(),
        ..proposed
    }
}

pub(super) fn apply_retention(items: &mut [IndexItem], state: Option<&UserMemoryLearningState>) {
    let Some(state) = state else {
        return;
    };
    for item in items {
        if let Some(record) = state
            .retention
            .get(&item.id)
            .filter(|record| record.content_digest == item.content_digest)
        {
            item.valid_to = Some(record.expires_at.clone());
        }
    }
}

pub(super) fn inactive_document_entries(
    settings: &super::UserMemorySettingsSnapshot,
    state: Option<&UserMemoryLearningState>,
) -> Vec<String> {
    let now = Utc::now();
    super::index_parse::build_index_snapshot(settings, state)
        .items
        .into_iter()
        .filter(|item| {
            item.kind == "memory"
                && item
                    .valid_to
                    .as_deref()
                    .is_some_and(|value| parse_time(value).is_ok_and(|expiry| expiry <= now))
        })
        .map(|item| item.id)
        .collect()
}

pub(super) fn validate_retention(
    records: &BTreeMap<String, MemoryRetention>,
) -> Result<(), AppCommandError> {
    if records.len() > MAX_RETENTION_RECORDS {
        return Err(AppCommandError::invalid_input(
            "Memory retirement record limit exceeded",
        ));
    }
    for (id, record) in records {
        if !(super::is_valid_memory_entry_id(id) || super::is_valid_experience_id(id))
            || !super::is_lower_hex_string(&record.content_digest, 64)
            || record.source_revision.is_empty()
            || record.source_revision.chars().count() > 128
            || record.reason.trim().is_empty()
            || record.reason.chars().count() > MAX_RETIRE_REASON_CHARS
            || contains_potential_secret(&record.reason)
        {
            return Err(AppCommandError::invalid_input(
                "Invalid memory retirement record",
            ));
        }
        let expiry = parse_time(&record.expires_at)?;
        if expiry.to_rfc3339_opts(SecondsFormat::Millis, true) != record.expires_at {
            return Err(AppCommandError::invalid_input(
                "Memory expiry must use canonical UTC milliseconds",
            ));
        }
        parse_time(&record.updated_at)?;
    }
    Ok(())
}

fn parse_time(value: &str) -> Result<DateTime<Utc>, AppCommandError> {
    DateTime::parse_from_rfc3339(value)
        .map(|time| time.with_timezone(&Utc))
        .map_err(|_| {
            AppCommandError::invalid_input(
                "Memory expiry must be an RFC3339 timestamp with timezone",
            )
        })
}
