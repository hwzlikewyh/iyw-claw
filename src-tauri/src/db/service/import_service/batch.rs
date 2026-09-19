use sea_orm::Condition;
use std::collections::{HashMap, HashSet};

use sea_orm::{
    ActiveValue::NotSet, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait,
    QueryFilter, Set, TransactionTrait,
};

use super::AutoTitleCandidate;
use crate::db::entities::conversation::{self, ConversationTitleSource};
use crate::db::error::DbError;
use crate::models::{AgentType, ConversationSummary};

type SessionKey = (String, String);

#[derive(Default)]
pub(super) struct BatchResult {
    pub imported: u32,
    pub skipped: u32,
    pub titles: Vec<AutoTitleCandidate>,
}

pub(super) async fn import_batch(
    conn: &DatabaseConnection,
    folder_id: i32,
    items: &[(AgentType, ConversationSummary)],
) -> Result<BatchResult, DbError> {
    let txn = conn.begin().await?;
    let mut existing = load_existing(&txn, items).await?;
    let mut inserted = HashSet::new();
    let mut models = Vec::new();
    for (agent, summary) in items {
        let key = session_key(*agent, summary);
        if !existing.contains_key(&key) && inserted.insert(key) {
            models.push(imported_model(folder_id, *agent, summary));
        }
    }
    if !models.is_empty() {
        // 查重与插入共享快照；并发写入导致的 BUSY 由调用方回滚后重试。
        conversation::Entity::insert_many(models)
            .exec_without_returning(&txn)
            .await?;
        existing = load_existing(&txn, items).await?;
    }
    let result = classify(items, existing, inserted);
    txn.commit().await?;
    Ok(result)
}

fn session_key(agent: AgentType, summary: &ConversationSummary) -> SessionKey {
    (agent.as_wire().into_owned(), summary.id.clone())
}

async fn load_existing<C: ConnectionTrait>(
    conn: &C,
    items: &[(AgentType, ConversationSummary)],
) -> Result<HashMap<SessionKey, conversation::Model>, DbError> {
    let keys = items
        .iter()
        .fold(Condition::any(), |condition, (agent, item)| {
            condition.add(
                Condition::all()
                    .add(conversation::Column::ExternalId.eq(&item.id))
                    .add(conversation::Column::AgentType.eq(agent.as_wire().into_owned())),
            )
        });
    let rows = conversation::Entity::find().filter(keys).all(conn).await?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let key = (row.agent_type.clone(), row.external_id.clone()?);
            Some((key, row))
        })
        .collect())
}

fn classify(
    items: &[(AgentType, ConversationSummary)],
    existing: HashMap<SessionKey, conversation::Model>,
    mut inserted: HashSet<SessionKey>,
) -> BatchResult {
    let mut result = BatchResult::default();
    for (agent, summary) in items {
        let key = session_key(*agent, summary);
        if inserted.remove(&key) {
            result.imported += 1;
            continue;
        }
        let title = existing
            .get(&key)
            .and_then(|row| title_candidate(row, summary));
        match title {
            Some(title) => result.titles.push(title),
            None => result.skipped += 1,
        }
    }
    result
}

fn title_candidate(
    row: &conversation::Model,
    summary: &ConversationSummary,
) -> Option<AutoTitleCandidate> {
    if row.parent_id.is_some() || row.deleted_at.is_some() || row.title_locked {
        return None;
    }
    let title = summary.title.as_deref()?.trim();
    if title.is_empty() || row.title.as_deref() == Some(title) {
        return None;
    }
    Some(AutoTitleCandidate {
        conversation_id: row.id,
        title: title.to_owned(),
    })
}

fn imported_model(
    folder_id: i32,
    agent_type: AgentType,
    summary: &ConversationSummary,
) -> conversation::ActiveModel {
    let created_at = summary.started_at;
    let updated_at = summary.ended_at.unwrap_or(created_at);
    conversation::ActiveModel {
        id: NotSet,
        folder_id: Set(folder_id),
        title: Set(summary.title.clone()),
        title_locked: Set(false),
        title_source: Set(ConversationTitleSource::UserFallback),
        title_summary_attempted: Set(false),
        agent_type: Set(agent_type.as_wire().into_owned()),
        status: Set(conversation::ConversationStatus::Completed),
        kind: Set(conversation::ConversationKind::Regular),
        model: Set(summary.model.clone()),
        git_branch: Set(summary.git_branch.clone()),
        external_id: Set(Some(summary.id.clone())),
        parent_id: Set(None),
        parent_tool_use_id: Set(None),
        delegation_call_id: Set(None),
        message_count: Set(summary.message_count as i32),
        last_completed_turn_generation: Set(0),
        created_at: Set(created_at),
        updated_at: Set(updated_at),
        deleted_at: Set(None),
        pinned_at: Set(None),
    }
}
