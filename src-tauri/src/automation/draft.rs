use std::time::Duration;

use tokio::sync::broadcast::error::RecvError;

use crate::acp::error::AcpError;
use crate::acp::manager::ConnectionManager;
use crate::acp::types::{AcpEvent, PromptInputBlock};
use crate::acp::InternalEventBus;
use crate::app_error::AppCommandError;
use crate::commands::acp::{build_session_runtime_env, verify_agent_installed};
use crate::commands::conversations::get_folder_conversation_core;
use crate::db::service::folder_service;
use crate::db::AppDatabase;
use crate::models::{AgentType, ContentBlock, TurnRole};
use crate::web::event_bridge::EventEmitter;

const DRAFT_TIMEOUT_SECS: u64 = 180;
const DRAFT_CONTEXT_MAX_CHARS: usize = 24_000;
const DRAFT_RESULT_MAX_CHARS: usize = 12_000;

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationDraftSource {
    pub name: String,
    pub agent_type: AgentType,
    pub root_folder_id: i32,
    pub prompt: String,
}

pub struct DraftRuntime<'a> {
    pub db: &'a AppDatabase,
    pub manager: &'a ConnectionManager,
    pub bus: &'a InternalEventBus,
    pub emitter: EventEmitter,
    pub data_dir: &'a std::path::Path,
}

struct SummarizeInput<'a> {
    agent_type: AgentType,
    working_dir: &'a str,
    context: String,
}

pub async fn create_from_conversation(
    runtime: DraftRuntime<'_>,
    conversation_id: i32,
) -> Result<AutomationDraftSource, AppCommandError> {
    tracing::info!(conversation_id, "[automation-draft] request started");
    let (detail, _) = get_folder_conversation_core(&runtime.db.conn, conversation_id).await?;
    let folder = folder_service::get_folder_by_id(&runtime.db.conn, detail.summary.folder_id)
        .await
        .map_err(AppCommandError::from)?
        .ok_or_else(|| AppCommandError::not_found("Conversation folder not found"))?;
    let context = render_conversation(&detail.turns);
    if context.trim().is_empty() {
        return Err(AppCommandError::invalid_input(
            "Conversation has no reusable text content",
        ));
    }

    let prompt = summarize_with_agent(
        &runtime,
        SummarizeInput {
            agent_type: detail.summary.agent_type,
            working_dir: &folder.path,
            context,
        },
    )
    .await?;

    tracing::info!(
        conversation_id,
        prompt_chars = prompt.chars().count(),
        "[automation-draft] request completed"
    );

    Ok(AutomationDraftSource {
        name: detail
            .summary
            .title
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_default(),
        agent_type: detail.summary.agent_type,
        root_folder_id: detail.summary.folder_id,
        prompt,
    })
}

async fn summarize_with_agent(
    runtime: &DraftRuntime<'_>,
    input: SummarizeInput<'_>,
) -> Result<String, AppCommandError> {
    let runtime_env =
        build_session_runtime_env(runtime.db, input.agent_type, None, runtime.data_dir)
            .await
            .map_err(agent_error)?;
    verify_agent_installed(input.agent_type, &runtime_env).map_err(agent_error)?;
    tracing::info!(
        agent = ?input.agent_type,
        working_dir = input.working_dir,
        context_chars = input.context.chars().count(),
        "[automation-draft] summarizer starting"
    );
    let mut receiver = runtime.bus.subscribe();
    let connection_id = runtime
        .manager
        .spawn_agent(
            input.agent_type,
            Some(input.working_dir.to_string()),
            None,
            runtime_env,
            "automation-draft".to_string(),
            runtime.emitter.clone(),
            None,
            Default::default(),
        )
        .await
        .map_err(agent_error)?;
    let prompt = draft_prompt(&input.context);
    if let Err(error) = runtime
        .manager
        .send_prompt(
            &connection_id,
            vec![PromptInputBlock::Text { text: prompt }],
        )
        .await
    {
        let _ = runtime.manager.disconnect(&connection_id).await;
        tracing::warn!(
            connection_id = %connection_id,
            error = %error,
            "[automation-draft] prompt failed"
        );
        return Err(agent_error(error));
    }

    let result = wait_for_draft(runtime.manager, &mut receiver, &connection_id).await;
    let _ = runtime.manager.disconnect(&connection_id).await;
    if let Err(error) = &result {
        tracing::warn!(
            connection_id = %connection_id,
            error = %error,
            "[automation-draft] summarizer failed"
        );
    }
    result
}

async fn wait_for_draft(
    manager: &ConnectionManager,
    receiver: &mut tokio::sync::broadcast::Receiver<std::sync::Arc<crate::acp::EventEnvelope>>,
    connection_id: &str,
) -> Result<String, AppCommandError> {
    let wait = async {
        loop {
            match receiver.recv().await {
                Ok(event) if event.connection_id == connection_id => match &event.payload {
                    AcpEvent::TurnComplete { stop_reason, .. } if stop_reason == "end_turn" => {
                        let text = final_text(manager, connection_id).await?;
                        return Ok(truncate(&text, DRAFT_RESULT_MAX_CHARS));
                    }
                    AcpEvent::TurnComplete { stop_reason, .. } => {
                        return Err(AppCommandError::task_execution_failed(
                            "Agent could not summarize this conversation",
                        )
                        .with_detail(format!("stop reason: {stop_reason}")));
                    }
                    AcpEvent::Error {
                        message, terminal, ..
                    } if *terminal => {
                        return Err(AppCommandError::task_execution_failed(
                            "Agent could not summarize this conversation",
                        )
                        .with_detail(message.clone()));
                    }
                    _ => {}
                },
                Ok(_) | Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => {
                    return Err(AppCommandError::task_execution_failed(
                        "Automation draft event stream closed",
                    ));
                }
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(DRAFT_TIMEOUT_SECS), wait)
        .await
        .map_err(|_| {
            tracing::warn!(
                connection_id = %connection_id,
                timeout_secs = DRAFT_TIMEOUT_SECS,
                "[automation-draft] summarizer timed out"
            );
            AppCommandError::task_execution_failed(
                "Agent timed out while creating automation draft",
            )
        })?
}

async fn final_text(
    manager: &ConnectionManager,
    connection_id: &str,
) -> Result<String, AppCommandError> {
    let state = manager.get_state(connection_id).await.ok_or_else(|| {
        AppCommandError::task_execution_failed("Automation draft connection closed early")
    })?;
    let text = state
        .read()
        .await
        .last_assistant_text
        .clone()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppCommandError::task_execution_failed("Agent returned an empty draft"));
    text
}

fn render_conversation(turns: &[crate::models::MessageTurn]) -> String {
    let mut rendered = String::new();
    for turn in turns {
        let role = match turn.role {
            TurnRole::User => "USER",
            TurnRole::Assistant => "ASSISTANT",
            TurnRole::System => continue,
        };
        let text = turn
            .blocks
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text.trim()),
                _ => None,
            })
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        if !text.is_empty() {
            rendered.push_str(role);
            rendered.push_str(":\n");
            rendered.push_str(&text);
            rendered.push_str("\n\n");
        }
    }
    bound_context(&rendered, DRAFT_CONTEXT_MAX_CHARS)
}

fn draft_prompt(context: &str) -> String {
    format!(
        "将下面已完成的对话整理为一段可重复执行的自动化任务说明。\n\
         只输出任务说明正文，不要标题、前言、代码围栏或解释。\n\
         使用命令式表述，保留明确目标、约束、验收标准和用户后续修正；\n\
         删除一次性进度、寒暄、时间点和已经失效的尝试；不得编造对话中没有的要求。\n\
         这一步只整理，不要执行任务，也不要修改任何文件。\n\n\
         <conversation>\n{context}\n</conversation>"
    )
}

fn truncate(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

fn bound_context(text: &str, max_chars: usize) -> String {
    let chars = text.chars().collect::<Vec<_>>();
    if chars.len() <= max_chars {
        return text.to_string();
    }
    let head_len = max_chars / 3;
    let tail_len = max_chars - head_len;
    let head = chars[..head_len].iter().collect::<String>();
    let tail = chars[chars.len() - tail_len..].iter().collect::<String>();
    format!("{head}\n\n[中间较早内容已省略]\n\n{tail}")
}

fn agent_error(error: AcpError) -> AppCommandError {
    AppCommandError::task_execution_failed("Failed to create automation draft")
        .with_detail(error.to_string())
}
