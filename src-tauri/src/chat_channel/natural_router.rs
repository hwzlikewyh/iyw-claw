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

    // Agent-judged routing: for a fresh message with no session context, let
    // the managed LLM router pick the folder and agent from the message
    // itself (zero user commands). Runs whenever the app is signed in (the
    // router rides the built-in model gateway); any error or low-confidence
    // verdict falls through to the deterministic heuristics below.
    let channel_agent = channel_default_agent(db, channel_id).await;
    match super::natural_router_config::get_runtime_config(db).await {
        Ok(Some(config)) => {
            match super::llm_router::route_with_llm(db, &config, trimmed, lang, channel_agent)
                .await
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

/// The agent configured on the channel itself (settings → 消息渠道 → 默认
/// Agent, stored as `default_agent_type` inside the channel's config JSON).
/// Sits between the sender's explicit `/agent` choice and the folder default
/// in the resolution chain.
pub async fn channel_default_agent(
    db: &DatabaseConnection,
    channel_id: i32,
) -> Option<AgentType> {
    let channel = crate::db::service::chat_channel_service::get_by_id(db, channel_id)
        .await
        .ok()
        .flatten()?;
    let config: serde_json::Value = serde_json::from_str(&channel.config_json).ok()?;
    let value = config.get("default_agent_type")?.as_str()?;
    parse_agent_type(value)
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
                "要开始新任务，请发送 /new <任务描述>。已有会话时可以直接发消息继续。".to_string()
            }
            _ => {
                "To start a new task, send /new <task>. Existing conversations can continue with plain text."
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};
    use std::time::Instant;

    use crate::acp::types::PermissionOptionInfo;
    use crate::chat_channel::session_bridge::{ActiveSession, PendingPermission};
    use crate::db::test_helpers::fresh_in_memory_db;

    #[test]
    fn approval_detection_accepts_chinese_and_english() {
        assert!(is_approval(&normalize("可以，继续")));
        assert!(is_approval(&normalize("ok proceed")));
        assert!(!is_approval(&normalize("先别动")));
    }

    #[test]
    fn denial_detection_accepts_chinese_and_english() {
        assert!(is_denial(&normalize("不行，拒绝")));
        assert!(is_denial(&normalize("no stop")));
        assert!(!is_denial(&normalize("继续处理")));
    }

    #[test]
    fn infers_agent_from_text() {
        assert_eq!(infer_agent_type("让 codex 跑一下"), Some(AgentType::Codex));
        assert_eq!(
            infer_agent_type("use claude code for this"),
            Some(AgentType::ClaudeCode)
        );
        assert_eq!(infer_agent_type("正常处理这个任务"), None);
    }

    #[test]
    fn matches_folder_name_or_path_part() {
        assert!(text_matches_folder(
            "帮我看 iyw-claw 的构建问题",
            "iyw-claw",
            "D:/projects/iyw-claw"
        ));
        assert!(!text_matches_folder(
            "排查 projects 下面的那个项目",
            "other",
            "D:/projects/other"
        ));
        assert!(!text_matches_folder(
            "处理登录问题",
            "billing",
            "D:/apps/billing"
        ));
    }

    #[tokio::test]
    async fn starts_task_in_only_workspace_when_no_session_exists() {
        let db = fresh_in_memory_db().await;
        let folder_id = crate::db::test_helpers::seed_folder(&db, "D:/projects/iyw-claw").await;
        let bridge = Arc::new(Mutex::new(SessionBridge::new()));

        let decision = route_natural_message(
            &db.conn,
            &bridge,
            1,
            "user-a",
            "帮我看一下 iyw-claw 为什么 CPU 很高",
            Lang::ZhCn,
        )
        .await;

        assert_eq!(
            decision,
            NaturalRouteDecision::StartTask {
                task: "帮我看一下 iyw-claw 为什么 CPU 很高".to_string(),
                folder_id,
                agent_type: AgentType::Codex,
            }
        );
    }

    #[tokio::test]
    async fn asks_clarification_when_no_workspace_exists() {
        let db = fresh_in_memory_db().await;
        let bridge = Arc::new(Mutex::new(SessionBridge::new()));

        let decision =
            route_natural_message(&db.conn, &bridge, 1, "user-a", "帮我修一下", Lang::ZhCn).await;

        assert!(matches!(
            decision,
            NaturalRouteDecision::AskClarification { .. }
        ));
    }

    #[tokio::test]
    async fn starts_task_in_most_recent_workspace_when_multiple_exist() {
        let db = fresh_in_memory_db().await;
        crate::db::test_helpers::seed_folder(&db, "D:/projects/alpha").await;
        crate::db::test_helpers::seed_folder(&db, "D:/projects/beta").await;
        let bridge = Arc::new(Mutex::new(SessionBridge::new()));

        let decision =
            route_natural_message(&db.conn, &bridge, 1, "user-a", "帮我修一下", Lang::ZhCn).await;

        // Command-free chat: plain text must start a task (in the most
        // recently opened workspace) rather than ask the user to run /new.
        assert!(matches!(
            decision,
            NaturalRouteDecision::StartTask { .. }
        ));
    }

    #[tokio::test]
    async fn active_session_takes_plain_text_as_followup() {
        let db = fresh_in_memory_db().await;
        let bridge = Arc::new(Mutex::new(SessionBridge::new()));
        bridge
            .lock()
            .await
            .register("conn-1".to_string(), active_session(1, "user-a", None));

        let decision =
            route_natural_message(&db.conn, &bridge, 1, "user-a", "继续修", Lang::ZhCn).await;

        assert_eq!(decision, NaturalRouteDecision::ContinueSession);
    }

    #[tokio::test]
    async fn existing_sender_conversation_takes_plain_text_as_followup() {
        let db = fresh_in_memory_db().await;
        let channel = crate::db::service::chat_channel_service::create(
            &db.conn,
            "test".to_string(),
            "weixin".to_string(),
            "{}".to_string(),
            true,
            false,
            None,
        )
        .await
        .unwrap();
        let folder_id = crate::db::test_helpers::seed_folder(&db, "D:/projects/iyw-claw").await;
        let conversation_id =
            crate::db::test_helpers::seed_conversation(&db, folder_id, AgentType::Codex).await;
        crate::db::service::sender_context_service::update_folder(
            &db.conn,
            channel.id,
            "user-a",
            Some(folder_id),
        )
        .await
        .unwrap();
        crate::db::service::sender_context_service::update_session(
            &db.conn,
            channel.id,
            "user-a",
            Some(conversation_id),
            None,
        )
        .await
        .unwrap();
        let bridge = Arc::new(Mutex::new(SessionBridge::new()));

        let decision =
            route_natural_message(&db.conn, &bridge, channel.id, "user-a", "你好", Lang::ZhCn)
                .await;

        assert_eq!(decision, NaturalRouteDecision::ContinueSession);
    }

    #[tokio::test]
    async fn explicit_project_without_existing_conversation_starts_new_task() {
        let db = fresh_in_memory_db().await;
        let channel = crate::db::service::chat_channel_service::create(
            &db.conn,
            "test".to_string(),
            "lark".to_string(),
            "{}".to_string(),
            true,
            false,
            None,
        )
        .await
        .unwrap();
        let alpha_id = crate::db::test_helpers::seed_folder(&db, "D:/projects/alpha").await;
        let beta_id = crate::db::test_helpers::seed_folder(&db, "D:/projects/beta").await;
        let bridge = Arc::new(Mutex::new(SessionBridge::new()));
        crate::db::service::sender_context_service::update_folder(
            &db.conn,
            channel.id,
            "user-a",
            Some(alpha_id),
        )
        .await
        .unwrap();

        let decision = route_natural_message(
            &db.conn,
            &bridge,
            channel.id,
            "user-a",
            "帮我看一下 beta 的测试",
            Lang::ZhCn,
        )
        .await;

        assert_eq!(
            decision,
            NaturalRouteDecision::StartTask {
                task: "帮我看一下 beta 的测试".to_string(),
                folder_id: beta_id,
                agent_type: AgentType::Codex,
            }
        );
    }

    #[tokio::test]
    async fn current_sender_folder_starts_task_when_no_conversation_exists() {
        let db = fresh_in_memory_db().await;
        let channel = crate::db::service::chat_channel_service::create(
            &db.conn,
            "test".to_string(),
            "weixin".to_string(),
            "{}".to_string(),
            true,
            false,
            None,
        )
        .await
        .unwrap();
        let alpha_id = crate::db::test_helpers::seed_folder(&db, "D:/projects/alpha").await;
        crate::db::test_helpers::seed_folder(&db, "D:/projects/beta").await;
        let bridge = Arc::new(Mutex::new(SessionBridge::new()));
        crate::db::service::sender_context_service::update_folder(
            &db.conn,
            channel.id,
            "user-a",
            Some(alpha_id),
        )
        .await
        .unwrap();

        let decision =
            route_natural_message(&db.conn, &bridge, channel.id, "user-a", "你好", Lang::ZhCn)
                .await;

        assert_eq!(
            decision,
            NaturalRouteDecision::StartTask {
                task: "你好".to_string(),
                folder_id: alpha_id,
                agent_type: AgentType::Codex,
            }
        );
    }

    #[tokio::test]
    async fn ambiguous_explicit_project_without_existing_conversation_asks_for_new_command() {
        let db = fresh_in_memory_db().await;
        crate::db::test_helpers::seed_folder(&db, "D:/projects/api").await;
        crate::db::test_helpers::seed_folder(&db, "D:/archives/api").await;
        let bridge = Arc::new(Mutex::new(SessionBridge::new()));

        let decision =
            route_natural_message(&db.conn, &bridge, 1, "user-a", "帮我看 api", Lang::ZhCn).await;

        assert!(matches!(
            decision,
            NaturalRouteDecision::AskClarification { ref message }
                if message.contains("/new")
        ));
    }

    #[tokio::test]
    async fn active_session_can_be_cancelled_with_plain_text() {
        let db = fresh_in_memory_db().await;
        let bridge = Arc::new(Mutex::new(SessionBridge::new()));
        bridge
            .lock()
            .await
            .register("conn-1".to_string(), active_session(1, "user-a", None));

        let decision =
            route_natural_message(&db.conn, &bridge, 1, "user-a", "取消这个任务", Lang::ZhCn).await;

        assert_eq!(decision, NaturalRouteDecision::CancelSession);
    }

    #[tokio::test]
    async fn pending_permission_maps_approval_text_to_approve() {
        let db = fresh_in_memory_db().await;
        let bridge = Arc::new(Mutex::new(SessionBridge::new()));
        bridge.lock().await.register(
            "conn-1".to_string(),
            active_session(
                1,
                "user-a",
                Some(PendingPermission {
                    request_id: "req-1".to_string(),
                    tool_description: "Bash: cargo test".to_string(),
                    options: vec![PermissionOptionInfo {
                        option_id: "allow".to_string(),
                        name: "Allow".to_string(),
                        kind: "allow".to_string(),
                    }],
                    sent_message_id: None,
                }),
            ),
        );

        let decision =
            route_natural_message(&db.conn, &bridge, 1, "user-a", "可以，继续", Lang::ZhCn).await;

        assert_eq!(
            decision,
            NaturalRouteDecision::ApprovePermission { always: false }
        );
    }

    #[tokio::test]
    async fn approve_for_current_session_does_not_enable_persistent_auto_approve() {
        let db = fresh_in_memory_db().await;
        let bridge = Arc::new(Mutex::new(SessionBridge::new()));
        bridge.lock().await.register(
            "conn-1".to_string(),
            active_session(
                1,
                "user-a",
                Some(PendingPermission {
                    request_id: "req-1".to_string(),
                    tool_description: "Bash: cargo test".to_string(),
                    options: vec![PermissionOptionInfo {
                        option_id: "allow".to_string(),
                        name: "Allow".to_string(),
                        kind: "allow".to_string(),
                    }],
                    sent_message_id: None,
                }),
            ),
        );

        let decision =
            route_natural_message(&db.conn, &bridge, 1, "user-a", "本会话可以", Lang::ZhCn).await;

        assert_eq!(
            decision,
            NaturalRouteDecision::ApprovePermission { always: false }
        );
    }

    #[test]
    fn search_history_keyword_extraction_is_case_insensitive() {
        assert_eq!(
            search_keyword(
                "Search History login bug",
                &normalize("Search History login bug")
            ),
            Some("login bug".to_string())
        );
    }

    fn active_session(
        channel_id: i32,
        sender_id: &str,
        permission_pending: Option<PendingPermission>,
    ) -> ActiveSession {
        ActiveSession {
            channel_id,
            sender_id: sender_id.to_string(),
            target: crate::chat_channel::types::ChannelMessageTarget::channel(channel_id),
            conversation_id: 1,
            connection_id: "conn-1".to_string(),
            agent_type: AgentType::Codex,
            content_buffer: String::new(),
            tool_calls: Vec::new(),
            tool_call_inputs: HashMap::new(),
            delegation_rendered: HashSet::new(),
            last_flushed: Instant::now(),
            pending_prompt: None,
            permission_pending,
        }
    }
}
