use std::collections::{HashMap, HashSet};

use sea_orm::{ColumnTrait, DatabaseTransaction, EntityTrait, QueryFilter};

use crate::acp::{AgentInputStatus, AgentInputStrategy};
use crate::db::entities::agent_input_outbox;
use crate::db::error::DbError;

use super::agent_input_ordering_service::FreezePrefixRequest;

pub(super) fn validate_unique(ids: &[String]) -> Result<(), DbError> {
    if ids.iter().collect::<HashSet<_>>().len() != ids.len() {
        return Err(validation("agent input order contains duplicate ids"));
    }
    Ok(())
}

pub(super) fn movable_rows(rows: &[agent_input_outbox::Model]) -> Vec<&agent_input_outbox::Model> {
    rows.iter().filter(|row| is_movable(row)).collect()
}

fn is_movable(row: &agent_input_outbox::Model) -> bool {
    row.force_batch_id.is_none()
        && matches!(
            AgentInputStatus::parse(&row.status),
            Some(
                AgentInputStatus::Waiting
                    | AgentInputStatus::FallbackQueued
                    | AgentInputStatus::Failed
            )
        )
}

pub(super) fn validate_membership(
    movable: &[&agent_input_outbox::Model],
    ordered_ids: &[String],
) -> Result<(), DbError> {
    let expected = movable.iter().map(|row| &row.id).collect::<HashSet<_>>();
    let provided = ordered_ids.iter().collect::<HashSet<_>>();
    if expected != provided {
        return Err(validation(
            "agent input order must contain every movable item exactly once",
        ));
    }
    Ok(())
}

pub(super) fn validate_locked_boundaries(
    rows: &[agent_input_outbox::Model],
    movable: &[&agent_input_outbox::Model],
    ordered_ids: &[String],
) -> Result<(), DbError> {
    let mut segment = 0usize;
    let mut segments = HashMap::new();
    for row in rows {
        if is_movable(row) {
            segments.insert(row.id.as_str(), segment);
        } else {
            segment = segment.saturating_add(1);
        }
    }
    if movable
        .iter()
        .zip(ordered_ids)
        .any(|(slot, id)| segments.get(slot.id.as_str()) != segments.get(id.as_str()))
    {
        return Err(validation("agent input order cannot cross a locked item"));
    }
    Ok(())
}

pub(super) fn validate_force_target(
    rows: &[agent_input_outbox::Model],
    target_id: &str,
) -> Result<usize, DbError> {
    if rows.iter().any(|row| row.force_batch_id.is_some()) {
        return Err(validation("an agent input force batch is already active"));
    }
    rows.iter()
        .position(|row| row.id == target_id)
        .ok_or_else(|| validation("agent input target is no longer pending"))
}

pub(super) async fn load_expected_rows(
    txn: &DatabaseTransaction,
    request: &FreezePrefixRequest<'_>,
) -> Result<Vec<agent_input_outbox::Model>, DbError> {
    let rows = agent_input_outbox::Entity::find()
        .filter(agent_input_outbox::Column::Id.is_in(request.expected_prefix_ids.iter().cloned()))
        .all(txn)
        .await?;
    let invalid = rows.len() != request.expected_prefix_ids.len()
        || rows
            .iter()
            .any(|row| row.conversation_id != request.conversation_id || row.deleted_at.is_some());
    if invalid {
        return Err(validation(
            "agent input force prefix contains an unknown item",
        ));
    }
    Ok(rows)
}

pub(super) fn validate_prefix(
    rows: &[agent_input_outbox::Model],
    target: usize,
    expected_rows: &[agent_input_outbox::Model],
    request: &FreezePrefixRequest<'_>,
) -> Result<(), DbError> {
    let check = PrefixCheck {
        prefix: &rows[..=target],
        target_sort_index: rows[target].sort_index,
        expected_rows,
        request,
    };
    check.validate_order()?;
    check.validate_pending_members()?;
    check.validate_statuses()
}

struct PrefixCheck<'a, 'b> {
    prefix: &'a [agent_input_outbox::Model],
    target_sort_index: i64,
    expected_rows: &'a [agent_input_outbox::Model],
    request: &'a FreezePrefixRequest<'b>,
}

impl PrefixCheck<'_, '_> {
    fn expected_by_id(&self) -> HashMap<&str, &agent_input_outbox::Model> {
        self.expected_rows
            .iter()
            .map(|row| (row.id.as_str(), row))
            .collect()
    }

    fn validate_order(&self) -> Result<(), DbError> {
        let by_id = self.expected_by_id();
        let out_of_order =
            self.request.expected_prefix_ids.windows(2).any(|pair| {
                by_id[pair[0].as_str()].sort_index > by_id[pair[1].as_str()].sort_index
            });
        let after_target = self
            .expected_rows
            .iter()
            .any(|row| row.sort_index > self.target_sort_index);
        if self.request.expected_prefix_ids.last().map(String::as_str)
            != Some(self.request.target_id)
            || out_of_order
            || after_target
        {
            return Err(validation(
                "agent input force prefix is not an ordered prefix",
            ));
        }
        Ok(())
    }

    fn validate_pending_members(&self) -> Result<(), DbError> {
        let by_id = self.expected_by_id();
        let expected = self
            .request
            .expected_prefix_ids
            .iter()
            .filter(|id| by_id[id.as_str()].consumed_at.is_none())
            .map(String::as_str);
        if self.prefix.iter().map(|row| row.id.as_str()).eq(expected) {
            return Ok(());
        }
        Err(validation(
            "agent input order changed before force dispatch",
        ))
    }

    fn validate_statuses(&self) -> Result<(), DbError> {
        if self
            .prefix
            .iter()
            .any(|row| row.status == AgentInputStatus::Failed.as_str())
        {
            return Err(validation(
                "retry or delete failed agent inputs before force dispatch",
            ));
        }
        let incompatible = self.prefix.iter().any(|row| {
            row.status == AgentInputStatus::Dispatching.as_str()
                && row.strategy.as_deref() != Some(AgentInputStrategy::CooperativeFeedback.as_str())
        });
        if incompatible {
            return Err(validation(
                "wait for the active agent input dispatch before forcing",
            ));
        }
        Ok(())
    }
}

fn validation(message: &str) -> DbError {
    DbError::Validation(message.into())
}
