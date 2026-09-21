use super::authority_types::{MemoryHistorySource, MemorySourceConversation};
use super::{authority_sql as sql, index_types::IndexEvidence};
use crate::app_error::AppCommandError;
use sea_orm::DatabaseConnection;

pub(super) async fn history_sources(
    db: &DatabaseConnection,
    evidence: &[IndexEvidence],
) -> Result<Vec<MemoryHistorySource>, AppCommandError> {
    let mut sources = Vec::new();
    for source in evidence {
        let id = match source
            .conversation_id
            .as_deref()
            .and_then(|id| id.parse::<i32>().ok())
        {
            Some(id) => Some(id),
            None => harvest_conversation(db, source).await?,
        };
        let conversation = match id {
            Some(id) => conversation(db, id).await?,
            None => None,
        };
        let availability = if conversation.is_some() {
            "available"
        } else if id.is_some() {
            "deleted_or_missing"
        } else {
            "unresolved"
        }
        .into();
        sources.push(MemoryHistorySource {
            source_id: source.source_id.clone(),
            turn_nonce: source.turn_nonce,
            conversation,
            availability,
        });
    }
    Ok(sources)
}

async fn harvest_conversation(
    db: &DatabaseConnection,
    source: &IndexEvidence,
) -> Result<Option<i32>, AppCommandError> {
    if source.turn_nonce <= 0
        || !matches!(
            source.source_kind.as_str(),
            "agent_experience" | "candidate_observation"
        )
    {
        return Ok(None);
    }
    let rows = sql::rows(
        db,
        "SELECT DISTINCT conversation_id FROM memory_harvest_outbox WHERE turn_nonce=? LIMIT ?",
        vec![source.turn_nonce.into(), (MAX_SOURCE_CANDIDATES + 1).into()],
    )
    .await?;
    if rows.len() > MAX_SOURCE_CANDIDATES as usize {
        return Ok(None);
    }
    let mut matched = None;
    for row in rows {
        let id: String = sql::field(&row, "conversation_id")?;
        if super::harvest::derive_harvest_source_id(&id) != source.source_id {
            continue;
        }
        let Ok(id) = id.parse::<i32>() else { continue };
        if matched.is_some() {
            return Ok(None);
        }
        matched = Some(id);
    }
    Ok(matched)
}

const MAX_SOURCE_CANDIDATES: i32 = 512;

async fn conversation(
    db: &DatabaseConnection,
    id: i32,
) -> Result<Option<MemorySourceConversation>, AppCommandError> {
    let rows = sql::rows(db,
        "SELECT c.id,c.folder_id,c.agent_type,c.title FROM conversation c JOIN folder f ON f.id=c.folder_id WHERE c.id=? AND c.deleted_at IS NULL AND f.deleted_at IS NULL",
        vec![id.into()]).await?;
    let Some(row) = rows.first() else {
        return Ok(None);
    };
    let agent = sql::field::<String>(row, "agent_type")?;
    let Ok(agent_type) = serde_json::from_value(serde_json::Value::String(agent)) else {
        return Ok(None);
    };
    Ok(Some(MemorySourceConversation {
        id: sql::field(row, "id")?,
        folder_id: sql::field(row, "folder_id")?,
        agent_type,
        title: sql::field(row, "title")?,
    }))
}
