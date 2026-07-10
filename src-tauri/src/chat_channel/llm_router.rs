use reqwest::StatusCode;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::i18n::Lang;
use super::natural_router::NaturalRouteDecision;
use super::natural_router_config::ChatNaturalRouterRuntimeConfig;
use crate::app_error::AppCommandError;
use crate::db::service::folder_service;
use crate::models::agent::AgentType;

const MAX_FOLDER_CANDIDATES: usize = 12;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouterFolderCandidate {
    pub id: i32,
    pub name: String,
    pub path: String,
    pub default_agent_type: Option<AgentType>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouterContext {
    pub message: String,
    pub language: String,
    pub folders: Vec<RouterFolderCandidate>,
    pub available_agents: Vec<AgentType>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum LlmRouteAction {
    StartTask,
    ShowStatus,
    ShowToday,
    SearchHistory,
    AskClarification,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct LlmRouteOutput {
    action: LlmRouteAction,
    confidence: f32,
    task: Option<String>,
    folder_id: Option<i32>,
    agent_type: Option<AgentType>,
    keyword: Option<String>,
    message: Option<String>,
    reason: Option<String>,
}

pub async fn route_with_llm(
    db: &DatabaseConnection,
    config: &ChatNaturalRouterRuntimeConfig,
    text: &str,
    lang: Lang,
) -> Result<Option<NaturalRouteDecision>, AppCommandError> {
    let context = build_router_context(db, text, lang).await?;
    if context.folders.is_empty() {
        return Ok(Some(NaturalRouteDecision::AskClarification {
            message: no_workspace_message(lang),
        }));
    }

    let output = call_chat_completions(config, &context).await?;
    let decision = validate_llm_output(&output, &context, config.min_confidence, lang);

    tracing::info!(
        "[ChatChannel] llm natural route action={:?} confidence={} reason={:?} accepted={}",
        output.action,
        output.confidence,
        output.reason,
        decision.is_some()
    );

    Ok(decision)
}

pub async fn build_router_context(
    db: &DatabaseConnection,
    text: &str,
    lang: Lang,
) -> Result<RouterContext, AppCommandError> {
    let mut folders = folder_service::list_open_folders(db)
        .await
        .map_err(AppCommandError::from)?;
    if folders.is_empty() {
        folders = folder_service::list_folders(db)
            .await
            .map_err(AppCommandError::from)?;
    }

    let mut candidates = Vec::new();
    for entry in folders.into_iter().take(MAX_FOLDER_CANDIDATES) {
        let detail = folder_service::get_folder_by_id(db, entry.id)
            .await
            .map_err(AppCommandError::from)?;
        candidates.push(RouterFolderCandidate {
            id: entry.id,
            name: entry.name,
            path: entry.path,
            default_agent_type: detail.and_then(|f| f.default_agent_type),
        });
    }

    Ok(RouterContext {
        message: text.trim().to_string(),
        language: lang_code(lang).to_string(),
        folders: candidates,
        available_agents: vec![
            AgentType::Codex,
            AgentType::ClaudeCode,
            AgentType::OpenCode,
            AgentType::Gemini,
            AgentType::OpenClaw,
            AgentType::Cline,
            AgentType::Hermes,
            AgentType::CodeBuddy,
            AgentType::KimiCode,
            AgentType::Pi,
        ],
    })
}

async fn call_chat_completions(
    config: &ChatNaturalRouterRuntimeConfig,
    context: &RouterContext,
) -> Result<LlmRouteOutput, AppCommandError> {
    let client = reqwest::Client::builder()
        .timeout(config.timeout)
        .build()
        .map_err(|e| {
            AppCommandError::network("Failed to build router HTTP client")
                .with_detail(e.to_string())
        })?;

    let response = client
        .post(&config.api_url)
        .bearer_auth(&config.api_key)
        .json(&json!({
            "model": config.model,
            "temperature": 0,
            "max_tokens": 320,
            "response_format": {
                "type": "json_schema",
                "json_schema": route_json_schema(),
            },
            "messages": [
                {
                    "role": "system",
                    "content": router_system_prompt()
                },
                {
                    "role": "user",
                    "content": serde_json::to_string(context).unwrap_or_default()
                }
            ]
        }))
        .send()
        .await
        .map_err(|e| {
            AppCommandError::network("Router request failed").with_detail(e.to_string())
        })?;

    let status = response.status();
    let body = response.text().await.map_err(|e| {
        AppCommandError::network("Failed to read router response").with_detail(e.to_string())
    })?;

    if !status.is_success() {
        return Err(router_status_error(status, &body));
    }

    parse_chat_completion_response(&body)
}

fn router_system_prompt() -> &'static str {
    "You route plain chat-channel messages for a coding workbench. \
Return only JSON that matches the supplied schema. Choose one internal action. \
Do not invent folders or agents; use only the candidate folder IDs and agent enums. \
Prefer start_task for coding requests, show_today for day summaries, show_status for channel/runtime status, \
search_history when the user asks to find previous conversations, and ask_clarification when required context is genuinely ambiguous. \
The app executes the decision after validation; you never execute tools directly."
}

fn route_json_schema() -> Value {
    json!({
        "name": "chat_route_decision",
        "strict": true,
        "schema": {
            "type": "object",
            "additionalProperties": false,
            "required": [
                "action",
                "confidence",
                "task",
                "folder_id",
                "agent_type",
                "keyword",
                "message",
                "reason"
            ],
            "properties": {
                "action": {
                    "type": "string",
                    "enum": [
                        "start_task",
                        "show_status",
                        "show_today",
                        "search_history",
                        "ask_clarification"
                    ]
                },
                "confidence": {
                    "type": "number",
                    "minimum": 0,
                    "maximum": 1
                },
                "task": {
                    "type": ["string", "null"],
                    "description": "Task text for start_task; otherwise null."
                },
                "folder_id": {
                    "type": ["integer", "null"],
                    "description": "Candidate folder id for start_task; otherwise null."
                },
                "agent_type": {
                    "type": ["string", "null"],
                    "enum": [
                        "claude_code",
                        "codex",
                        "open_code",
                        "gemini",
                        "open_claw",
                        "cline",
                        "hermes",
                        "code_buddy",
                        "kimi_code",
                        "pi",
                        null
                    ],
                    "description": "Agent enum for start_task; null uses the folder default or Codex."
                },
                "keyword": {
                    "type": ["string", "null"],
                    "description": "Search keyword for search_history; otherwise null."
                },
                "message": {
                    "type": ["string", "null"],
                    "description": "Natural clarification text for ask_clarification; otherwise null."
                },
                "reason": {
                    "type": ["string", "null"],
                    "description": "Short internal reason for logs."
                }
            }
        }
    })
}

fn parse_chat_completion_response(body: &str) -> Result<LlmRouteOutput, AppCommandError> {
    let root: Value = serde_json::from_str(body).map_err(|e| {
        AppCommandError::configuration_invalid("Router response is not JSON")
            .with_detail(e.to_string())
    })?;
    let content = root
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            let refusal = root
                .pointer("/choices/0/message/refusal")
                .and_then(Value::as_str)
                .unwrap_or("missing message content");
            AppCommandError::configuration_invalid("Router response has no content")
                .with_detail(refusal.to_string())
        })?;

    parse_llm_route_output(content)
}

pub fn parse_llm_route_output(raw: &str) -> Result<LlmRouteOutput, AppCommandError> {
    serde_json::from_str(raw).map_err(|e| {
        AppCommandError::configuration_invalid("Router decision is invalid")
            .with_detail(e.to_string())
    })
}

pub fn validate_llm_output(
    output: &LlmRouteOutput,
    context: &RouterContext,
    min_confidence: f32,
    lang: Lang,
) -> Option<NaturalRouteDecision> {
    if !output.confidence.is_finite() || output.confidence < min_confidence {
        return None;
    }

    match output.action {
        LlmRouteAction::StartTask => {
            let task = non_empty(output.task.as_deref())?;
            let folder_id = output.folder_id?;
            let folder = context.folders.iter().find(|f| f.id == folder_id)?;
            let agent_type = output
                .agent_type
                .or(folder.default_agent_type)
                .unwrap_or(AgentType::Codex);
            Some(NaturalRouteDecision::StartTask {
                task: task.to_string(),
                folder_id,
                agent_type,
            })
        }
        LlmRouteAction::ShowStatus => Some(NaturalRouteDecision::ShowStatus),
        LlmRouteAction::ShowToday => Some(NaturalRouteDecision::ShowToday),
        LlmRouteAction::SearchHistory => Some(NaturalRouteDecision::SearchHistory {
            keyword: non_empty(output.keyword.as_deref())?.to_string(),
        }),
        LlmRouteAction::AskClarification => Some(NaturalRouteDecision::AskClarification {
            message: non_empty(output.message.as_deref())
                .map(str::to_string)
                .unwrap_or_else(|| clarification_message(lang)),
        }),
    }
}

fn router_status_error(status: StatusCode, body: &str) -> AppCommandError {
    let detail = body.chars().take(500).collect::<String>();
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            AppCommandError::authentication_failed("Router API authentication failed")
                .with_detail(detail)
        }
        StatusCode::TOO_MANY_REQUESTS => {
            AppCommandError::network("Router API rate limited").with_detail(detail)
        }
        _ => AppCommandError::network(format!("Router API returned HTTP {status}"))
            .with_detail(detail),
    }
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|v| !v.is_empty())
}

fn clarification_message(lang: Lang) -> String {
    match lang {
        Lang::ZhCn | Lang::ZhTw => "我需要再确认一下你想处理哪个项目或动作。".to_string(),
        _ => "I need one more detail about which project or action you want.".to_string(),
    }
}

fn no_workspace_message(lang: Lang) -> String {
    match lang {
        Lang::ZhCn | Lang::ZhTw => {
            "我还没有可用的项目上下文。请直接说项目名或先在 iyw-claw 打开一个项目。".to_string()
        }
        _ => "I do not have a workspace context yet. Mention the project name or open one in iyw-claw first.".to_string(),
    }
}

fn lang_code(lang: Lang) -> &'static str {
    match lang {
        Lang::ZhCn => "zh-CN",
        Lang::ZhTw => "zh-TW",
        Lang::En => "en",
        Lang::Ja => "ja",
        Lang::Ko => "ko",
        Lang::Es => "es",
        Lang::De => "de",
        Lang::Fr => "fr",
        Lang::Pt => "pt",
        Lang::Ar => "ar",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> RouterContext {
        RouterContext {
            message: "帮我修 iyw-claw 的测试".to_string(),
            language: "zh-CN".to_string(),
            folders: vec![RouterFolderCandidate {
                id: 7,
                name: "iyw-claw".to_string(),
                path: "D:/projects/iyw-claw".to_string(),
                default_agent_type: Some(AgentType::ClaudeCode),
            }],
            available_agents: vec![AgentType::Codex, AgentType::ClaudeCode],
        }
    }

    #[test]
    fn parses_valid_llm_route_output() {
        let output = parse_llm_route_output(
            r#"{
                "action":"start_task",
                "confidence":0.91,
                "task":"帮我修测试",
                "folder_id":7,
                "agent_type":"codex",
                "keyword":null,
                "message":null,
                "reason":"matched folder name"
            }"#,
        )
        .expect("parse");

        assert_eq!(output.action, LlmRouteAction::StartTask);
        assert_eq!(output.folder_id, Some(7));
        assert_eq!(output.agent_type, Some(AgentType::Codex));
    }

    #[test]
    fn validates_start_task_against_folder_candidates() {
        let output = LlmRouteOutput {
            action: LlmRouteAction::StartTask,
            confidence: 0.9,
            task: Some("帮我修测试".to_string()),
            folder_id: Some(7),
            agent_type: None,
            keyword: None,
            message: None,
            reason: None,
        };

        assert_eq!(
            validate_llm_output(&output, &context(), 0.72, Lang::ZhCn),
            Some(NaturalRouteDecision::StartTask {
                task: "帮我修测试".to_string(),
                folder_id: 7,
                agent_type: AgentType::ClaudeCode,
            })
        );
    }

    #[test]
    fn rejects_low_confidence_or_unknown_folder() {
        let mut output = LlmRouteOutput {
            action: LlmRouteAction::StartTask,
            confidence: 0.4,
            task: Some("帮我修测试".to_string()),
            folder_id: Some(7),
            agent_type: Some(AgentType::Codex),
            keyword: None,
            message: None,
            reason: None,
        };
        assert!(validate_llm_output(&output, &context(), 0.72, Lang::ZhCn).is_none());

        output.confidence = 0.9;
        output.folder_id = Some(99);
        assert!(validate_llm_output(&output, &context(), 0.72, Lang::ZhCn).is_none());
    }

    #[test]
    fn extracts_chat_completion_content() {
        let body = r#"{
            "choices":[{
                "message":{
                    "content":"{\"action\":\"show_status\",\"confidence\":0.88,\"task\":null,\"folder_id\":null,\"agent_type\":null,\"keyword\":null,\"message\":null,\"reason\":\"status request\"}"
                }
            }]
        }"#;

        let output = parse_chat_completion_response(body).expect("parse response");
        assert_eq!(output.action, LlmRouteAction::ShowStatus);
    }
}
