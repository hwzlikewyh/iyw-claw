use std::sync::Arc;

use sea_orm::DatabaseConnection;
use tokio::sync::Mutex;

use super::i18n::Lang;
use super::session_bridge::SessionBridge;
use crate::db::service::{folder_service, sender_context_service};
use crate::models::agent::AgentType;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NaturalRouteDecision {
    ContinueSession,
    ApprovePermission {
        always: bool,
    },
    DenyPermission,
    CancelSession,
    StartTask {
        task: String,
        folder_id: i32,
        agent_type: AgentType,
    },
    ShowStatus,
    ShowToday,
    SearchHistory {
        keyword: String,
    },
    AskClarification {
        message: String,
    },
}

pub async fn route_natural_message(
    db: &DatabaseConnection,
    bridge: &Arc<Mutex<SessionBridge>>,
    channel_id: i32,
    sender_id: &str,
    text: &str,
    lang: Lang,
) -> NaturalRouteDecision {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return NaturalRouteDecision::AskClarification {
            message: clarification_message(lang),
        };
    }

    let normalized = normalize(trimmed);
    let sender_has_session = has_active_session(bridge, channel_id, sender_id).await;

    if has_pending_permission(bridge, channel_id, sender_id).await {
        if is_denial(&normalized) {
            return NaturalRouteDecision::DenyPermission;
        }
        if is_approval(&normalized) {
            return NaturalRouteDecision::ApprovePermission {
                always: is_approve_always(&normalized),
            };
        }
    }

    if sender_has_session {
        if is_cancel_session(&normalized) {
            return NaturalRouteDecision::CancelSession;
        }
        return NaturalRouteDecision::ContinueSession;
    }

    if sender_has_conversation(db, channel_id, sender_id).await {
        if is_cancel_session(&normalized) {
            return NaturalRouteDecision::CancelSession;
        }
        return NaturalRouteDecision::ContinueSession;
    }

    if is_status_query(&normalized) {
        return NaturalRouteDecision::ShowStatus;
    }
    if is_today_query(&normalized) {
        return NaturalRouteDecision::ShowToday;
    }
    if let Some(keyword) = search_keyword(trimmed, &normalized) {
        return NaturalRouteDecision::SearchHistory { keyword };
    }

    // Channel-dedicated workspace: if this channel has its own folder (set at
    // creation time), route every fresh task there directly — no heuristics,
    // no ambiguity. The user never has to type /folder or /new.
    let channel_agent = channel_default_agent(db, channel_id).await;
    if let Some(folder_id) = channel_dedicated_folder(db, channel_id).await {
        let sender_agent = sender_context_service::get_or_create(db, channel_id, sender_id)
            .await
            .ok()
            .and_then(|ctx| ctx.current_agent_type);
        let agent_type = infer_agent_type(trimmed)
            .or_else(|| sender_agent.as_deref().and_then(parse_agent_type))
            .or(channel_agent)
            .unwrap_or(AgentType::Codex);
        return NaturalRouteDecision::StartTask {
            task: trimmed.to_string(),
            folder_id,
            agent_type,
        };
    }

    // Agent-judged routing: for a fresh message with no session context, let
    // the managed LLM router pick the folder and agent from the message
    // itself (zero user commands). Runs whenever the app is signed in (the
    // router rides the built-in model gateway); any error or low-confidence
    // verdict falls through to the deterministic heuristics below.
    match super::natural_router_config::get_runtime_config(db).await {
        Ok(Some(config)) => {
            match super::llm_router::route_with_llm(db, &config, trimmed, lang, channel_agent).await
            {
                Ok(Some(decision)) => return decision,
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(
                        "[ChatChannel] llm router unavailable, using heuristics: {error}"
                    );
                }
            }
        }
        Ok(None) => {}
        Err(error) => {
            tracing::warn!("[ChatChannel] llm router config load failed: {error}");
        }
    }

    if let Some(decision) =
        start_task_from_available_context(db, channel_id, sender_id, trimmed, &normalized).await
    {
        return decision;
    }

    NaturalRouteDecision::AskClarification {
        message: no_existing_conversation_message(db, lang).await,
    }
}

pub fn agent_type_to_wire(agent_type: AgentType) -> String {
    serde_json::to_value(agent_type)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| "codex".to_string())
}

/// Build a brief recent-history preamble for dedicated-folder channels.
///
/// Finds the channel's workspace root, lists its dated subfolders, collects
/// conversation titles from the past 7 days, and returns a compact summary
/// string the dispatcher can prepend to the initial prompt so the agent
/// has continuity without the user having to re-explain context.
///
/// Returns `None` if the channel has no workspace root or no prior sessions.
pub async fn build_channel_memory_context(
    db: &DatabaseConnection,
    channel_id: i32,
    lang: Lang,
) -> Option<String> {
    let channel = crate::db::service::chat_channel_service::get_by_id(db, channel_id)
        .await
        .ok()
        .flatten()?;
    let config: serde_json::Value = serde_json::from_str(&channel.config_json).ok()?;
    let root = config.get("channel_workspace_root")?.as_str()?;

    // Enumerate dated sub-directories that actually exist in the workspace.
    let read_dir = std::fs::read_dir(root).ok()?;
    let mut day_entries: Vec<String> = read_dir
        .filter_map(|entry| {
            let name = entry.ok()?.file_name().to_string_lossy().to_string();
            // Keep only YYYY-MM-DD directories.
            if name.len() == 10 && name.chars().nth(4) == Some('-') {
                Some(name)
            } else {
                None
            }
        })
        .collect();
    day_entries.sort_by(|a, b| b.cmp(a)); // newest first
    day_entries.truncate(7);

    // For each day, find the matching folder row then its conversation titles.
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let mut lines: Vec<String> = Vec::new();
    for day in &day_entries {
        if day == &today {
            continue; // skip today — it's the current session
        }
        let day_path = format!("{root}/{day}");
        // Normalize path separators so the lookup matches what add_folder stored.
        let day_path_norm = day_path.replace('/', std::path::MAIN_SEPARATOR_STR);

        // Find the folder row by path prefix match (list_folders returns all).
        let all_folders = crate::db::service::folder_service::list_folders(db)
            .await
            .unwrap_or_default();
        let folder_id = all_folders
            .iter()
            .find(|f| f.path == day_path_norm || f.path == day_path)
            .map(|f| f.id);

        let Some(fid) = folder_id else { continue };

        let convs = crate::db::service::conversation_service::list_by_folder(
            db, fid, None, None, None, None,
        )
        .await
        .unwrap_or_default();

        let titles: Vec<&str> = convs
            .iter()
            .filter_map(|c| c.title.as_deref())
            .take(3)
            .collect();
        if !titles.is_empty() {
            lines.push(format!("• {day}: {}", titles.join(" / ")));
        }
    }

    if lines.is_empty() {
        return None;
    }

    let header = match lang {
        Lang::ZhCn | Lang::ZhTw => "[近期工作记录]\n",
        _ => "[Recent sessions]\n",
    };
    let footer = match lang {
        Lang::ZhCn | Lang::ZhTw => "---\n",
        _ => "---\n",
    };
    Some(format!("{header}{}\n{footer}", lines.join("\n")))
}
/// Agent, stored as `default_agent_type` inside the channel's config JSON).
/// Sits between the sender's explicit `/agent` choice and the folder default
/// in the resolution chain.
pub async fn channel_default_agent(db: &DatabaseConnection, channel_id: i32) -> Option<AgentType> {
    let channel = crate::db::service::chat_channel_service::get_by_id(db, channel_id)
        .await
        .ok()
        .flatten()?;
    let config: serde_json::Value = serde_json::from_str(&channel.config_json).ok()?;
    let value = config.get("default_agent_type")?.as_str()?;
    parse_agent_type(value)
}

/// Returns the `default_folder_id` stored in the channel's config JSON, if
/// any. Set automatically at channel-creation time so that every message on
/// this channel routes to a dedicated workspace without any folder-matching
/// heuristics.
async fn channel_dedicated_folder(db: &DatabaseConnection, channel_id: i32) -> Option<i32> {
    let channel = crate::db::service::chat_channel_service::get_by_id(db, channel_id)
        .await
        .ok()
        .flatten()?;
    let config: serde_json::Value = serde_json::from_str(&channel.config_json).ok()?;

    // New-style: daily subfolders under a persistent root path.
    if let Some(root) = config
        .get("channel_workspace_root")
        .and_then(|v| v.as_str())
    {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let today_path = std::path::PathBuf::from(root).join(&today);
        if let Err(e) = std::fs::create_dir_all(&today_path) {
            tracing::warn!(
                "[channel_dedicated_folder] failed to create daily dir {}: {e}",
                today_path.display()
            );
            return None;
        }
        return folder_service::add_folder(db, &today_path.to_string_lossy())
            .await
            .ok()
            .map(|f| f.id);
    }

    // Legacy: static folder_id written by older installs.
    config
        .get("default_folder_id")
        .and_then(|v| v.as_i64())
        .map(|v| v as i32)
}

async fn has_active_session(
    bridge: &Arc<Mutex<SessionBridge>>,
    channel_id: i32,
    sender_id: &str,
) -> bool {
    let guard = bridge.lock().await;
    guard.find_by_sender(channel_id, sender_id).is_some()
}

async fn has_pending_permission(
    bridge: &Arc<Mutex<SessionBridge>>,
    channel_id: i32,
    sender_id: &str,
) -> bool {
    let guard = bridge.lock().await;
    guard
        .find_by_sender(channel_id, sender_id)
        .and_then(|s| s.permission_pending.as_ref())
        .is_some()
}

async fn sender_has_conversation(
    db: &DatabaseConnection,
    channel_id: i32,
    sender_id: &str,
) -> bool {
    sender_context_service::get_or_create(db, channel_id, sender_id)
        .await
        .ok()
        .and_then(|ctx| ctx.current_conversation_id)
        .is_some()
}

async fn no_existing_conversation_message(db: &DatabaseConnection, lang: Lang) -> String {
    let has_workspace = folder_service::list_open_folders(db)
        .await
        .map(|folders| !folders.is_empty())
        .unwrap_or(false)
        || folder_service::list_folders(db)
            .await
            .map(|folders| !folders.is_empty())
            .unwrap_or(false);

    if has_workspace {
        match lang {
            Lang::ZhCn | Lang::ZhTw => {
                "找到多个匹配的项目，请在消息中说明项目名，比如「在 xxx 项目里...」".to_string()
            }
            _ => {
                "Found multiple matching projects. Please mention the project name, e.g. \"in the xxx project...\""
                    .to_string()
            }
        }
    } else {
        no_workspace_message(lang)
    }
}

fn infer_agent_type(text: &str) -> Option<AgentType> {
    let normalized = normalize(text);
    let checks: &[(AgentType, &[&str])] = &[
        (AgentType::Codex, &["codex", "openai"]),
        (AgentType::ClaudeCode, &["claude", "claude code"]),
        (AgentType::OpenCode, &["opencode", "open code"]),
        (AgentType::Gemini, &["gemini"]),
        (AgentType::OpenClaw, &["openclaw", "open claw"]),
        (AgentType::Cline, &["cline"]),
        (AgentType::Hermes, &["hermes"]),
        (AgentType::CodeBuddy, &["codebuddy", "code buddy"]),
        (AgentType::KimiCode, &["kimi", "kimi code"]),
        (AgentType::Pi, &[" pi ", "pi agent"]),
    ];

    checks
        .iter()
        .find(|(_, aliases)| aliases.iter().any(|alias| normalized.contains(alias)))
        .map(|(agent, _)| *agent)
}

fn text_matches_folder(text: &str, name: &str, path: &str) -> bool {
    let haystack = normalize(text);
    let name = normalize(name);
    if !name.is_empty() && haystack.contains(&name) {
        return true;
    }
    path_basename(path)
        .map(|part| haystack.contains(&normalize(part)))
        .unwrap_or(false)
}

fn path_basename(path: &str) -> Option<&str> {
    path.split(['/', '\\']).rfind(|part| !part.is_empty())
}

async fn start_task_from_available_context(
    db: &DatabaseConnection,
    channel_id: i32,
    sender_id: &str,
    task: &str,
    normalized: &str,
) -> Option<NaturalRouteDecision> {
    let folders = available_folders(db).await;
    if folders.is_empty() {
        return None;
    }

    let explicit_matches = folders
        .iter()
        .filter(|folder| text_matches_folder(normalized, &folder.name, &folder.path))
        .collect::<Vec<_>>();

    let folder_id = if explicit_matches.len() == 1 {
        explicit_matches[0].id
    } else if explicit_matches.len() > 1 {
        return None;
    } else {
        let ctx = sender_context_service::get_or_create(db, channel_id, sender_id)
            .await
            .ok();
        ctx.and_then(|ctx| ctx.current_folder_id)
            // Zero-friction IM chat: when nothing resolves explicitly, fall
            // back to the most recently opened workspace instead of asking
            // the user to run /folder first (`available_folders` is ordered
            // most-recent-first). The task reply names the folder, so a wrong
            // guess is visible and correctable via /folder.
            .or_else(|| folders.first().map(|folder| folder.id))?
    };

    let folder = folder_service::get_folder_by_id(db, folder_id)
        .await
        .ok()
        .flatten()?;
    let sender_agent = sender_context_service::get_or_create(db, channel_id, sender_id)
        .await
        .ok()
        .and_then(|ctx| ctx.current_agent_type);
    let channel_agent = channel_default_agent(db, channel_id).await;
    let agent_type = infer_agent_type(task)
        .or_else(|| sender_agent.as_deref().and_then(parse_agent_type))
        .or(channel_agent)
        .or(folder.default_agent_type)
        .unwrap_or(AgentType::Codex);

    Some(NaturalRouteDecision::StartTask {
        task: task.to_string(),
        folder_id,
        agent_type,
    })
}

async fn available_folders(db: &DatabaseConnection) -> Vec<crate::models::FolderHistoryEntry> {
    let open = folder_service::list_open_folders(db)
        .await
        .unwrap_or_default();
    if !open.is_empty() {
        return open;
    }
    folder_service::list_folders(db).await.unwrap_or_default()
}

fn parse_agent_type(value: &str) -> Option<AgentType> {
    serde_json::from_value(serde_json::Value::String(value.to_string())).ok()
}

fn normalize(text: &str) -> String {
    format!(
        " {} ",
        text.to_lowercase().replace(['，', '。', '！', '？'], " ")
    )
}

fn is_approval(normalized: &str) -> bool {
    approval_terms()
        .iter()
        .any(|term| normalized.contains(term))
}

fn is_denial(normalized: &str) -> bool {
    denial_terms().iter().any(|term| normalized.contains(term))
}

fn is_cancel_session(normalized: &str) -> bool {
    let english_terms = [" cancel ", " stop ", " end session ", " cancel session "];
    let chinese_terms = ["取消", "停止", "结束", "终止", "别跑了"];
    english_terms.iter().any(|term| normalized.contains(term))
        || chinese_terms.iter().any(|term| normalized.contains(term))
}

fn is_approve_always(normalized: &str) -> bool {
    ["always", "以后都", "一直", "总是", "永久"]
        .iter()
        .any(|term| normalized.contains(term))
}

fn approval_terms() -> &'static [&'static str] {
    &[
        " approve ",
        " approved ",
        " allow ",
        " yes ",
        " ok ",
        " okay ",
        " continue ",
        " proceed ",
        " 可以 ",
        "可以",
        " 同意 ",
        "同意",
        " 批准 ",
        "批准",
        " 继续 ",
        "继续",
        " 好的 ",
        "好的",
        " 行 ",
        " 没问题 ",
        "没问题",
    ]
}

fn denial_terms() -> &'static [&'static str] {
    &[
        " deny ",
        " denied ",
        " reject ",
        " no ",
        " stop ",
        " cancel ",
        " 不行 ",
        " 不可以 ",
        " 拒绝 ",
        " 不要 ",
        " 停止 ",
        " 取消 ",
    ]
}

fn is_status_query(normalized: &str) -> bool {
    [" status ", " 状态 ", " 当前状态 ", " 渠道状态 "]
        .iter()
        .any(|term| normalized.contains(term))
}

fn is_today_query(normalized: &str) -> bool {
    [
        " today ",
        " 今天 ",
        " 今日 ",
        " 今天做了什么 ",
        " 今日总结 ",
    ]
    .iter()
    .any(|term| normalized.contains(term))
}

fn search_keyword(raw: &str, normalized: &str) -> Option<String> {
    for prefix in ["搜索历史", "查历史", "查会话", "search history"] {
        if normalized.contains(&normalize(prefix)) {
            let raw_lower = raw.to_lowercase();
            let prefix_lower = prefix.to_lowercase();
            let keyword = raw_lower
                .find(&prefix_lower)
                .map(|idx| {
                    let end = idx + prefix.len();
                    format!("{}{}", &raw[..idx], &raw[end..])
                })
                .unwrap_or_else(|| raw.replace(prefix, ""))
                .trim()
                .to_string();
            if !keyword.is_empty() {
                return Some(keyword);
            }
        }
    }
    None
}

fn clarification_message(lang: Lang) -> String {
    match lang {
        Lang::ZhCn | Lang::ZhTw => "你想让我处理什么任务？直接描述即可。".to_string(),
        _ => "What would you like me to handle? Describe the task directly.".to_string(),
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
