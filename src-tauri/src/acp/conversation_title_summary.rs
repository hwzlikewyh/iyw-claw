use std::time::Duration;

use sea_orm::DatabaseConnection;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::chat_channel::manager::ChatChannelManager;
use crate::commands::conversation_title::{self, ConversationTitleContext};
use crate::db::service::conversation_title_service;
use crate::web::event_bridge::EventEmitter;

const TITLE_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_INPUT_CHARS: usize = 4_000;
const MAX_TITLE_CHARS: usize = 100;
const TITLE_MAX_OUTPUT_TOKENS: u32 = 80;
const PRIVATE_ARTIFACT_HEADING: &str = "## Current-turn final artifact delivery";

pub(crate) use crate::acp::session_state::CompletedTurnTitleInput;

struct SummaryTask {
    conn: DatabaseConnection,
    emitter: EventEmitter,
    chat_channel_manager: ChatChannelManager,
    conversation_id: i32,
    input: CompletedTurnTitleInput,
    config: crate::acp::model_gateway_chat::ModelGatewayChatConfig,
}

pub(crate) async fn schedule_first_turn_summary(
    context: &ConversationTitleContext<'_>,
    conversation_id: i32,
    input: CompletedTurnTitleInput,
) -> Result<(), crate::app_error::AppCommandError> {
    let Some(model) = resolve_title_model(input.preferred_model.as_deref()) else {
        return Ok(());
    };
    let Some(config) =
        crate::acp::model_gateway_chat::runtime_config(context.conn, model, TITLE_TIMEOUT).await?
    else {
        return Ok(());
    };
    if !conversation_title_service::claim_summary_attempt(context.conn, conversation_id).await? {
        return Ok(());
    }
    let task = SummaryTask {
        conn: context.conn.clone(),
        emitter: context.emitter.clone(),
        chat_channel_manager: context.chat_channel_manager.clone_ref(),
        conversation_id,
        input,
        config,
    };
    tokio::spawn(async move {
        let conversation_id = task.conversation_id;
        if let Err(error) = summarize(task).await {
            tracing::warn!(
                conversation_id,
                error = %error,
                "[conversation-title] Codex summary generation failed"
            );
        }
    });
    Ok(())
}

pub(crate) fn is_private_title_candidate(title: &str) -> bool {
    let title = title.trim_start();
    title.starts_with(PRIVATE_ARTIFACT_HEADING)
        || title.starts_with(crate::user_memory::USER_CONTEXT_START)
}

async fn summarize(task: SummaryTask) -> Result<(), crate::app_error::AppCommandError> {
    let request = crate::acp::model_gateway_chat::StructuredChatRequest {
        system_prompt: system_prompt(),
        user_content: title_input_json(task.input),
        json_schema: title_schema(),
        max_tokens: TITLE_MAX_OUTPUT_TOKENS,
        operation: "Conversation title summary",
    };
    let raw = crate::acp::model_gateway_chat::call_structured(&task.config, request).await?;
    let title = parse_title(&raw)?;
    let context = ConversationTitleContext {
        conn: &task.conn,
        emitter: &task.emitter,
        chat_channel_manager: &task.chat_channel_manager,
    };
    if conversation_title::refresh_summary(&context, task.conversation_id, &title).await? {
        tracing::info!(
            conversation_id = task.conversation_id,
            title_chars = title.chars().count(),
            model = %task.config.model,
            "[conversation-title] Codex summary title applied"
        );
    }
    Ok(())
}

fn resolve_title_model(preferred: Option<&str>) -> Option<String> {
    let models = crate::acp::model_catalog::all_model_ids();
    preferred
        .and_then(|preferred| {
            models
                .iter()
                .find(|model| model.eq_ignore_ascii_case(preferred.trim()))
        })
        .or_else(|| models.first())
        .map(|model| (*model).to_string())
}

fn system_prompt() -> &'static str {
    "Create one concise conversation title from the real user request and the assistant's resolved outcome. Use the user's language. For Chinese use 6-20 Chinese characters; otherwise use 3-10 words. Return only the JSON schema. Do not use Markdown, quotes, trailing punctuation, generic labels, internal host instructions, or artifact-delivery wording."
}

fn title_input_json(input: CompletedTurnTitleInput) -> String {
    json!({
        "user_request": truncate(&input.user_text, MAX_INPUT_CHARS),
        "assistant_outcome": truncate(&input.assistant_text, MAX_INPUT_CHARS),
    })
    .to_string()
}

fn title_schema() -> Value {
    json!({
        "name": "conversation_title",
        "strict": true,
        "schema": {
            "type": "object",
            "additionalProperties": false,
            "required": ["title"],
            "properties": { "title": { "type": "string" } }
        }
    })
}

#[derive(Deserialize)]
struct TitleOutput {
    title: String,
}

fn parse_title(raw: &str) -> Result<String, crate::app_error::AppCommandError> {
    let output: TitleOutput = serde_json::from_str(raw).map_err(|error| {
        crate::app_error::AppCommandError::configuration_invalid(
            "Conversation title response is invalid",
        )
        .with_detail(error.to_string())
    })?;
    let normalized = output
        .title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let title = truncate(
        normalized.trim_matches(&['"', '\'', '#', ' '][..]),
        MAX_TITLE_CHARS,
    );
    if title.is_empty() || is_private_title_candidate(&title) {
        return Err(crate::app_error::AppCommandError::configuration_invalid(
            "Conversation title response is empty or private context",
        ));
    }
    Ok(title)
}

fn truncate(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let value = chars.by_ref().take(max_chars).collect::<String>();
    value.trim().to_string()
}
