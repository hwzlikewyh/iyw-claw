use futures::{stream, StreamExt};
use sea_orm::DatabaseConnection;
use tokio::sync::Semaphore;

use crate::app_error::AppCommandError;
use crate::models::{ContentBlock, DbConversationSummary, TurnRole};

const HISTORY_READ_CONCURRENCY: usize = 4;
static HISTORY_READ_SLOTS: Semaphore = Semaphore::const_new(HISTORY_READ_CONCURRENCY);

pub(super) async fn filter_conversations(
    conn: &DatabaseConnection,
    conversations: Vec<DbConversationSummary>,
    query: Option<&str>,
) -> Result<Vec<DbConversationSummary>, AppCommandError> {
    let (results, failures) = filter_page(conn, conversations, query).await?;
    if failures > 0 && results.is_empty() {
        return Err(AppCommandError::task_execution_failed(
            "Conversation history could not be searched completely",
        ));
    }
    Ok(results)
}

pub(super) async fn filter_page(
    conn: &DatabaseConnection,
    conversations: Vec<DbConversationSummary>,
    query: Option<&str>,
) -> Result<(Vec<DbConversationSummary>, usize), AppCommandError> {
    let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) else {
        return Ok((conversations, 0));
    };
    let query = query.to_lowercase();
    let started_at = std::time::Instant::now();
    let candidates = conversations.len();
    let mut pending = stream::iter(conversations.into_iter().map(|conversation| {
        let query = &query;
        async move {
            let matched = matches_conversation(conn, &conversation, query).await;
            (conversation, matched)
        }
    }))
    .buffered(HISTORY_READ_CONCURRENCY);
    let mut results = Vec::new();
    let mut failures = 0;
    while let Some((conversation, matched)) = pending.next().await {
        match matched {
            Ok(true) => results.push(conversation),
            Ok(false) => {}
            Err(error) => {
                failures += 1;
                tracing::warn!(conversation_id = conversation.id, error = %error,
                    "[conversation-search] history read failed");
            }
        }
    }
    tracing::debug!(
        candidates,
        matches = results.len(),
        failures,
        elapsed_ms = started_at.elapsed().as_millis(),
        "[conversation-search] completed"
    );
    Ok((results, failures))
}

async fn matches_conversation(
    conn: &DatabaseConnection,
    conversation: &DbConversationSummary,
    query: &str,
) -> Result<bool, AppCommandError> {
    if conversation
        .title
        .as_deref()
        .is_some_and(|title| title.to_lowercase().contains(query))
    {
        return Ok(true);
    }
    if conversation.external_id.is_none() {
        return Ok(false);
    }
    let _permit = HISTORY_READ_SLOTS.acquire().await.map_err(|error| {
        AppCommandError::task_execution_failed("Conversation search is unavailable")
            .with_detail(error.to_string())
    })?;
    // 读取完整历史并复用隐私上下文过滤，不能只搜索最近一页或任务摘要。
    let (detail, _) = super::get_folder_conversation_core(conn, conversation.id).await?;
    Ok(detail.turns.iter()
        .filter(|turn| matches!(turn.role, TurnRole::User | TurnRole::Assistant))
        .flat_map(|turn| &turn.blocks)
        .any(|block| matches!(block, ContentBlock::Text { text } if text.to_lowercase().contains(query))))
}
