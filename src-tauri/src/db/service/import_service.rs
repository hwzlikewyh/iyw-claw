use sea_orm::DatabaseConnection;

use crate::db::error::DbError;
use crate::models::{AgentType, ConversationSummary, ImportResult};
use crate::parsers::{history_parsers, path_eq_for_matching};

mod batch;

const IMPORT_BATCH_SIZE: usize = 32;

/// Import (and refresh the titles of) the local agent sessions under
/// `folder_path`. Returns the tally plus the ids of already-imported
/// conversations whose parsed title should be refreshed. The command layer
/// applies those candidates through the shared title coordinator so database,
/// sidebar, and chat-channel updates remain ordered with live Agent events and
/// manual renames.
pub async fn import_local_conversations(
    conn: &DatabaseConnection,
    folder_id: i32,
    folder_path: &str,
) -> Result<(ImportResult, Vec<AutoTitleCandidate>), DbError> {
    let path = folder_path.to_owned();
    let summaries = tokio::task::spawn_blocking(move || collect_local_sessions(&path))
        .await
        .map_err(|e| DbError::Migration(e.to_string()))?;

    let mut imported = 0u32;
    let mut skipped = 0u32;
    let mut title_candidates = Vec::new();

    for items in summaries.chunks(IMPORT_BATCH_SIZE) {
        let result = crate::db::retry_sqlite_maintenance("conversation.import", || {
            batch::import_batch(conn, folder_id, items)
        })
        .await?;
        imported += result.imported;
        skipped += result.skipped;
        title_candidates.extend(result.titles);
    }

    Ok((
        ImportResult {
            imported,
            updated: 0,
            skipped,
        },
        title_candidates,
    ))
}

#[derive(Debug, PartialEq, Eq)]
pub struct AutoTitleCandidate {
    pub conversation_id: i32,
    pub title: String,
}

fn collect_local_sessions(path: &str) -> Vec<(AgentType, ConversationSummary)> {
    let mut matched = Vec::new();
    for (agent, parser) in history_parsers() {
        match parser.list_conversations() {
            Ok(conversations) => matched.extend(conversations.into_iter().filter_map(|summary| {
                let matches = summary
                    .folder_path
                    .as_deref()
                    .is_some_and(|folder| path_eq_for_matching(folder, path));
                matches.then_some((agent, summary))
            })),
            Err(error) => tracing::error!(agent = ?agent, %error, "Error listing conversations"),
        }
    }
    matched
}
