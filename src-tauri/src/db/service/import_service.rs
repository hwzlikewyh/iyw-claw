use sea_orm::{
    ActiveModelTrait, ActiveValue::NotSet, ColumnTrait, DatabaseConnection, EntityTrait,
    QueryFilter, Set,
};

use crate::db::entities::conversation;
use crate::db::error::DbError;
use crate::models::{AgentType, ConversationSummary, ImportResult};
use crate::parsers::claude::ClaudeParser;
use crate::parsers::cline::ClineParser;
use crate::parsers::codebuddy::CodeBuddyParser;
use crate::parsers::codex::CodexParser;
use crate::parsers::gemini::GeminiParser;
use crate::parsers::grok::GrokParser;
use crate::parsers::hermes::HermesParser;
use crate::parsers::kimi_code::KimiCodeParser;
use crate::parsers::openclaw::OpenClawParser;
use crate::parsers::opencode::OpenCodeParser;
use crate::parsers::pi::PiParser;
use crate::parsers::{path_eq_for_matching, AgentParser};

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
    let path = folder_path.to_string();

    // Run parsers in blocking task since they do filesystem I/O
    let summaries = tokio::task::spawn_blocking(move || {
        let parsers: Vec<(AgentType, Box<dyn AgentParser>)> = vec![
            (AgentType::ClaudeCode, Box::new(ClaudeParser::new())),
            (AgentType::Codex, Box::new(CodexParser::new())),
            (AgentType::OpenCode, Box::new(OpenCodeParser::new())),
            (AgentType::Gemini, Box::new(GeminiParser::new())),
            (AgentType::OpenClaw, Box::new(OpenClawParser::new())),
            (AgentType::Cline, Box::new(ClineParser::new())),
            (AgentType::Hermes, Box::new(HermesParser::new())),
            (AgentType::CodeBuddy, Box::new(CodeBuddyParser::new())),
            (AgentType::KimiCode, Box::new(KimiCodeParser::new())),
            (AgentType::Pi, Box::new(PiParser::new())),
            (AgentType::Grok, Box::new(GrokParser::new())),
        ];

        let mut matched = Vec::new();
        for (at, parser) in &parsers {
            match parser.list_conversations() {
                Ok(convs) => {
                    for c in convs {
                        if c.folder_path
                            .as_deref()
                            .map(|p| path_eq_for_matching(p, path.as_str()))
                            .unwrap_or(false)
                        {
                            matched.push((*at, c));
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Error listing {} conversations: {}", at, e);
                }
            }
        }
        matched
    })
    .await
    .map_err(|e| DbError::Migration(e.to_string()))?;

    let mut imported = 0u32;
    let mut skipped = 0u32;
    let mut title_candidates = Vec::new();

    for (agent_type, summary) in &summaries {
        match import_one(conn, folder_id, agent_type, summary).await? {
            ImportOutcome::Imported => imported += 1,
            ImportOutcome::TitleCandidate(candidate) => title_candidates.push(candidate),
            ImportOutcome::Skipped => skipped += 1,
        }
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

/// Outcome of reconciling a single parsed session against the DB.
#[derive(Debug, PartialEq, Eq)]
enum ImportOutcome {
    /// A new conversation row was inserted.
    Imported,
    /// An already-imported conversation has a parsed title that the command
    /// layer should apply through the shared title coordinator.
    TitleCandidate(AutoTitleCandidate),
    /// Already imported, title left unchanged (locked, identical, or the parse
    /// produced no title).
    Skipped,
}

/// Insert a brand-new conversation, or — when it already exists — refresh its
/// title from the freshly parsed session file so an AI-generated title that did
/// not exist at first import is adopted. `refresh_auto_title` is a single
/// conditional UPDATE that skips locked or unchanged rows and never bumps
/// `updated_at`, so a re-import neither clobbers a manual rename nor reorders a
/// recency-sorted sidebar. A missing/empty parsed title leaves the existing
/// title intact rather than nulling it.
async fn import_one(
    conn: &DatabaseConnection,
    folder_id: i32,
    agent_type: &AgentType,
    summary: &ConversationSummary,
) -> Result<ImportOutcome, DbError> {
    let at_str = serde_json::to_value(agent_type)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default();

    let exists = conversation::Entity::find()
        .filter(conversation::Column::ExternalId.eq(&summary.id))
        .filter(conversation::Column::AgentType.eq(&at_str))
        .one(conn)
        .await?;

    if let Some(existing) = exists {
        // Preserve the original skip for rows the sidebar never shows: a
        // soft-deleted conversation must stay deleted (never resurrected or
        // rewritten), and a delegation child is not a sidebar row (the upsert
        // broadcast suppresses it too, which would also desync the `updated`
        // count). Only a visible root conversation gets its title refreshed.
        if existing.parent_id.is_some() || existing.deleted_at.is_some() {
            return Ok(ImportOutcome::Skipped);
        }
        if let Some(title) = summary
            .title
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
        {
            if !existing.title_locked && existing.title.as_deref() != Some(title) {
                return Ok(ImportOutcome::TitleCandidate(AutoTitleCandidate {
                    conversation_id: existing.id,
                    title: title.to_string(),
                }));
            }
        }
        return Ok(ImportOutcome::Skipped);
    }

    let created_at = summary.started_at;
    let updated_at = summary.ended_at.unwrap_or(created_at);
    let conv = conversation::ActiveModel {
        id: NotSet,
        folder_id: Set(folder_id),
        title: Set(summary.title.clone()),
        title_locked: Set(false),
        agent_type: Set(at_str),
        status: Set(conversation::ConversationStatus::Completed),
        // Imports scan regular folders' session files; chat scratch dirs and
        // loop runs are never import targets, so every imported row is regular.
        kind: Set(conversation::ConversationKind::Regular),
        model: Set(summary.model.clone()),
        git_branch: Set(summary.git_branch.clone()),
        external_id: Set(Some(summary.id.clone())),
        parent_id: Set(None),
        parent_tool_use_id: Set(None),
        delegation_call_id: Set(None),
        message_count: Set(summary.message_count as i32),
        created_at: Set(created_at),
        updated_at: Set(updated_at),
        deleted_at: Set(None),
        pinned_at: Set(None),
    };
    conv.insert(conn).await?;
    Ok(ImportOutcome::Imported)
}
