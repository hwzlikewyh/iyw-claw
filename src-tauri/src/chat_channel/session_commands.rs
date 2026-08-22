use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use sea_orm::DatabaseConnection;
use tokio::sync::Mutex;

use super::channel_context;
use super::i18n::{self, Lang};
use super::manager::ChatChannelManager;
use super::natural_router;
use super::session_bridge::{ActiveSession, SessionBridge, SessionOwnership};
pub use super::session_dispatch::{
    handle_post_action, CommandMessageResult, CommandPostAction, SessionCommandMessage,
};
use super::session_event_subscriber;
use super::session_picker::{chat_selectable_agents, is_chat_selectable_agent, parse_chat_agent};
pub use super::session_picker::{handle_agent_picker, handle_callback, handle_folder_picker};
use super::session_runtime;
use super::session_topic;
use super::session_topic_access;
use super::session_topic_messages;
use super::types::{ChannelMessageTarget, RichMessage};
use crate::acp::manager::ConnectionManager;
use crate::acp::types::PromptInputBlock;
use crate::commands::conversation_title::{self, ConversationTitleContext};
use crate::commands::conversations::{get_conversation_context_primer_core, ContextPrimerSource};
use crate::db::entities::conversation;
use crate::db::service::conversation_binding_service::ConversationRoute;
use crate::db::service::{
    conversation_binding_service, conversation_service, folder_service, sender_context_service,
};
use crate::models::agent::AgentType;
use crate::web::event_bridge::EventEmitter;

#[derive(Clone, Copy)]
pub struct FollowupRequest<'a> {
    pub db: &'a DatabaseConnection,
    pub text: &'a str,
    pub channel_id: i32,
    pub sender_id: &'a str,
    pub target: &'a ChannelMessageTarget,
    pub route: &'a ConversationRoute,
    pub manager: &'a ChatChannelManager,
    pub conn_mgr: &'a ConnectionManager,
    pub emitter: &'a EventEmitter,
    pub bridge: &'a Arc<Mutex<SessionBridge>>,
    pub data_dir: &'a Path,
    pub lang: Lang,
    pub prefix: &'a str,
    /// End-to-end trace id of the inbound message driving this follow-up.
    pub trace_id: Option<&'a str>,
}

// ── /folder ──

pub async fn handle_folder(
    db: &DatabaseConnection,
    args: &str,
    channel_id: i32,
    sender_id: &str,
    lang: Lang,
    prefix: &str,
) -> RichMessage {
    if args.is_empty() {
        return list_folders(db, channel_id, sender_id, lang, prefix).await;
    }

    // Try parse as index (1-based)
    if let Ok(idx) = args.parse::<usize>() {
        return select_folder_by_index(db, idx, channel_id, sender_id, lang, prefix).await;
    }

    // Treat as path
    select_folder_by_path(db, args, channel_id, sender_id, lang).await
}

async fn list_folders(
    db: &DatabaseConnection,
    channel_id: i32,
    sender_id: &str,
    lang: Lang,
    prefix: &str,
) -> RichMessage {
    let folders = match folder_service::list_folders(db).await {
        Ok(f) => f,
        Err(e) => {
            return RichMessage::error(format!("{}{e}", i18n::failed_to_list_folders_label(lang)));
        }
    };

    if folders.is_empty() {
        return RichMessage::info(i18n::no_folders_found(lang))
            .with_title(i18n::folder_title(lang));
    }

    let ctx = sender_context_service::get_or_create(db, channel_id, sender_id)
        .await
        .ok();

    let mut body = String::new();
    for (i, f) in folders.iter().take(10).enumerate() {
        let current = ctx
            .as_ref()
            .and_then(|c| c.current_folder_id)
            .map(|id| id == f.id)
            .unwrap_or(false);
        let marker = if current { " [*]" } else { "" };
        body.push_str(&format!("{}. {}{} ({})\n", i + 1, f.name, marker, f.path));
    }

    body.push_str(&format!("\n{}", i18n::folder_select_hint(lang, prefix)));

    RichMessage::info(body.trim_end()).with_title(i18n::folder_title(lang))
}

async fn select_folder_by_index(
    db: &DatabaseConnection,
    idx: usize,
    channel_id: i32,
    sender_id: &str,
    lang: Lang,
    prefix: &str,
) -> RichMessage {
    if idx == 0 {
        return RichMessage::info(i18n::index_starts_from_one(lang));
    }

    let folders = match folder_service::list_folders(db).await {
        Ok(f) => f,
        Err(e) => {
            return RichMessage::error(format!("{}{e}", i18n::failed_to_list_folders_label(lang)));
        }
    };

    let Some(folder) = folders.get(idx - 1) else {
        return RichMessage::info(i18n::folder_index_out_of_range(lang, prefix));
    };

    let _ = sender_context_service::update_folder(db, channel_id, sender_id, Some(folder.id)).await;

    RichMessage::info(format!("{} ({})", folder.name, folder.path))
        .with_title(i18n::folder_selected_title(lang))
}

async fn select_folder_by_path(
    db: &DatabaseConnection,
    path: &str,
    channel_id: i32,
    sender_id: &str,
    lang: Lang,
) -> RichMessage {
    let entry = match folder_service::add_folder(db, path).await {
        Ok(e) => e,
        Err(e) => {
            return RichMessage::error(format!("{}{e}", i18n::failed_to_add_folder_label(lang)));
        }
    };

    let _ = sender_context_service::update_folder(db, channel_id, sender_id, Some(entry.id)).await;

    RichMessage::info(format!("{} ({})", entry.name, entry.path))
        .with_title(i18n::folder_selected_title(lang))
}

// ── /agent ──

pub async fn handle_agent(
    db: &DatabaseConnection,
    args: &str,
    channel_id: i32,
    sender_id: &str,
    lang: Lang,
    prefix: &str,
) -> RichMessage {
    if args.is_empty() {
        return list_agents(db, channel_id, sender_id, lang, prefix).await;
    }

    // Try parse as index
    if let Ok(idx) = args.parse::<usize>() {
        return select_agent_by_index(db, idx, channel_id, sender_id, lang, prefix).await;
    }

    // Try parse as agent type name
    select_agent_by_name(db, args, channel_id, sender_id, lang).await
}

async fn list_agents(
    db: &DatabaseConnection,
    channel_id: i32,
    sender_id: &str,
    lang: Lang,
    prefix: &str,
) -> RichMessage {
    let agents = chat_selectable_agents();
    let ctx = sender_context_service::get_or_create(db, channel_id, sender_id)
        .await
        .ok();

    let mut body = String::new();
    for (i, at) in agents.iter().enumerate() {
        let at_str = agent_type_to_string(*at);
        let current = ctx
            .as_ref()
            .and_then(|c| c.current_agent_type.as_deref())
            .map(|s| s == at_str)
            .unwrap_or(false);
        let marker = if current { " [*]" } else { "" };
        body.push_str(&format!("{}. {}{}\n", i + 1, at, marker));
    }

    body.push_str(&format!("\n{}", i18n::agent_select_hint(lang, prefix)));

    RichMessage::info(body.trim_end()).with_title(i18n::agent_title(lang))
}

async fn select_agent_by_index(
    db: &DatabaseConnection,
    idx: usize,
    channel_id: i32,
    sender_id: &str,
    lang: Lang,
    prefix: &str,
) -> RichMessage {
    let agents = chat_selectable_agents();
    if idx == 0 || idx > agents.len() {
        return RichMessage::info(i18n::agent_index_out_of_range(lang, prefix));
    }

    let at = agents[idx - 1];
    let at_str = agent_type_to_string(at);
    let _ = sender_context_service::update_agent(db, channel_id, sender_id, Some(at_str)).await;

    RichMessage::info(at.to_string()).with_title(i18n::agent_selected_title(lang))
}

async fn select_agent_by_name(
    db: &DatabaseConnection,
    name: &str,
    channel_id: i32,
    sender_id: &str,
    lang: Lang,
) -> RichMessage {
    let at = match parse_agent_type(name) {
        Some(a) => a,
        None => {
            return RichMessage::info(format!("{}{}", i18n::unknown_agent_label(lang), name));
        }
    };
    if !is_chat_selectable_agent(at) {
        return RichMessage::info(format!("{}{}", i18n::unknown_agent_label(lang), name));
    }

    let at_str = agent_type_to_string(at);
    let _ = sender_context_service::update_agent(db, channel_id, sender_id, Some(at_str)).await;

    RichMessage::info(at.to_string()).with_title(i18n::agent_selected_title(lang))
}

// ── /task ──

#[allow(clippy::too_many_arguments)]
pub async fn handle_task(
    db: &DatabaseConnection,
    task_description: &str,
    channel_id: i32,
    sender_id: &str,
    target: &ChannelMessageTarget,
    route: &ConversationRoute,
    replacing_deleted: bool,
    replace_existing: bool,
    manager: &ChatChannelManager,
    conn_mgr: &ConnectionManager,
    emitter: &EventEmitter,
    bridge: &Arc<Mutex<SessionBridge>>,
    lang: Lang,
    prefix: &str,
    data_dir: &Path,
    trace_id: Option<&str>,
) -> CommandMessageResult {
    if task_description.is_empty() {
        return CommandMessageResult::current(
            RichMessage::info(i18n::task_usage(lang, prefix)),
            target,
        );
    }
    let target_available = session_topic_access::target_available_to_sender(db, target, sender_id)
        .await
        .unwrap_or(false);
    if !target_available {
        tracing::warn!(
            channel_id,
            route_key = %route.route_key,
            "[ChatChannel] rejected task replacement from non-owner"
        );
        return CommandMessageResult::current(
            RichMessage::info(session_topic_messages::active_session(lang, prefix)),
            target,
        );
    }
    if !replace_existing
        && session_topic::has_active_session(db, bridge, channel_id, &route.route_key).await
    {
        return CommandMessageResult::current(
            RichMessage::info(session_topic_messages::active_session(lang, prefix)),
            target,
        );
    }

    // 1. Load sender context
    let ctx = match sender_context_service::get_or_create(db, channel_id, sender_id).await {
        Ok(c) => c,
        Err(e) => {
            return CommandMessageResult::current(
                RichMessage::error(format!("{}{e}", i18n::failed_to_load_context_label(lang))),
                target,
            );
        }
    };

    let folder_id = match ctx.current_folder_id {
        Some(id) => id,
        None => {
            return CommandMessageResult::current(
                RichMessage::info(i18n::no_folder_selected(lang, prefix)),
                target,
            );
        }
    };

    // 2. Get folder info
    let folder = match folder_service::get_folder_by_id(db, folder_id).await {
        Ok(Some(f)) => f,
        _ => {
            return CommandMessageResult::current(
                RichMessage::info(i18n::folder_not_found_with_hint(lang, prefix)),
                target,
            );
        }
    };

    // 3. Resolve agent type: sender's explicit /agent choice → channel's
    // configured default agent → folder default.
    let channel_agent = natural_router::channel_default_agent(db, channel_id).await;
    let agent_type = match resolve_agent_type(
        &ctx.current_agent_type,
        channel_agent,
        &folder.default_agent_type,
    ) {
        Some(at) => at,
        None => {
            return CommandMessageResult::current(
                RichMessage::info(i18n::no_agent_selected(lang, prefix)),
                target,
            );
        }
    };

    let runtime_env = match session_runtime::build_runtime_env(db, agent_type, None, data_dir).await
    {
        Ok(runtime_env) => runtime_env,
        Err(error) => {
            return CommandMessageResult::current(
                RichMessage::error(format!(
                    "{}{error}",
                    i18n::failed_to_start_agent_label(lang)
                )),
                target,
            )
        }
    };

    let session_target = if target.is_telegram_general_topic() {
        match manager
            .create_thread(
                channel_id,
                &session_topic_messages::topic_title(task_description),
            )
            .await
        {
            Ok(created) => created,
            Err(error) => {
                return CommandMessageResult::current(
                    RichMessage::error(session_topic_messages::create_failed(
                        lang,
                        &error.to_string(),
                    )),
                    target,
                )
            }
        }
    } else {
        target.clone()
    };

    // 4. Create conversation record
    let conv = match conversation_service::create(
        db,
        folder_id,
        agent_type,
        Some(truncate_title(task_description)),
        folder.git_branch.clone(),
    )
    .await
    {
        Ok(c) => c,
        Err(e) => {
            return CommandMessageResult::current(
                RichMessage::error(format!(
                    "{}{e}",
                    i18n::failed_to_create_conversation_label(lang)
                )),
                target,
            );
        }
    };

    let previous_binding =
        conversation_binding_service::find_by_route(db, channel_id, &route.route_key)
            .await
            .ok()
            .flatten()
            .map(|binding| (binding.target_id, binding.conversation_id));

    let task_prompt =
        match channel_context::attach(db, channel_id, &route.target_id, task_description).await {
            Ok(prompt) => prompt,
            Err(error) => {
                let _ = conversation_service::update_status(
                    db,
                    conv.id,
                    conversation::ConversationStatus::Cancelled,
                )
                .await;
                return CommandMessageResult::current(
                    RichMessage::error(format!("可信渠道上下文加载失败：{error}")),
                    target,
                );
            }
        };

    // 5. Spawn ACP agent
    let owner_label = session_runtime::owner_label(channel_id, sender_id, &session_target);
    let connection_id = match conn_mgr
        .spawn_agent(
            agent_type,
            Some(folder.path.clone()),
            None,
            runtime_env,
            owner_label,
            emitter.clone(),
            None,
            BTreeMap::new(),
        )
        .await
    {
        Ok(id) => id,
        Err(e) => {
            // Clean up the conversation record
            let _ = conversation_service::update_status(
                db,
                conv.id,
                conversation::ConversationStatus::Cancelled,
            )
            .await;
            return CommandMessageResult::current(
                RichMessage::error(format!("{}{e}", i18n::failed_to_start_agent_label(lang))),
                target,
            );
        }
    };

    if session_target.is_telegram_forum_topic() {
        if let Err(error) = session_topic::bind_target(
            db,
            &session_target,
            conv.id,
            Some(connection_id.clone()),
            sender_id,
            conv.title.clone(),
        )
        .await
        {
            let _ = conn_mgr.cancel(db, &connection_id).await;
            let _ = conversation_service::update_status(
                db,
                conv.id,
                conversation::ConversationStatus::Cancelled,
            )
            .await;
            return CommandMessageResult::current(
                RichMessage::error(format!("Failed to bind Telegram topic: {error}")),
                target,
            );
        }
        let title_context = ConversationTitleContext {
            conn: db,
            emitter,
            chat_channel_manager: manager,
        };
        conversation_title::sync_channels(&title_context, conv.id).await;
    }

    // Record transient ownership before exposing the session to the event
    // subscriber. A fast bind failure can then clear only this connection.
    if !session_target.is_telegram_forum_topic() {
        let _ = sender_context_service::update_session(
            db,
            channel_id,
            sender_id,
            Some(conv.id),
            Some(connection_id.clone()),
        )
        .await;
    }

    // Register in bridge (prompt will be sent after SessionStarted event).
    let registration_generation = {
        let session = ActiveSession {
            channel_id,
            sender_id: sender_id.to_string(),
            target: session_target.clone(),
            route_key: route.route_key.clone(),
            target_id: route.target_id.clone(),
            bind_on_start: true,
            conversation_id: conv.id,
            connection_id: connection_id.clone(),
            ownership: SessionOwnership::ChannelOwned,
            registration_generation: 0,
            restoring_external_id: None,
            expected_external_id: None,
            observed_session_id: None,
            agent_type,
            content_buffer: String::new(),
            tool_calls: Vec::new(),
            tool_call_inputs: std::collections::HashMap::new(),
            delegation_rendered: std::collections::HashSet::new(),
            last_flushed: Instant::now(),
            pending_prompt: None,
            recovery_prompt: None,
            pending_prompt_attempts: 0,
            trace_id: trace_id.map(|s| s.to_string()),
            permission_pending: None,
        };
        SessionBridge::register_serialized(bridge, connection_id.clone(), session).await
    };
    session_event_subscriber::catch_up_session_start(bridge, manager, conn_mgr, db, &connection_id)
        .await;
    if bridge.lock().await.get(&connection_id).is_none() {
        return CommandMessageResult::current(RichMessage::info(""), &session_target);
    }

    CommandMessageResult {
        // 会话已在后台创建；不向外部渠道暴露 Agent、会话号或工作区等内核元数据。
        // 保留 post_action，让真正的任务提示和后续 Agent 回复继续发送。
        message: replacement_notice(lang, replacing_deleted),
        response_target: session_target.clone(),
        extra_responses: Vec::new(),
        post_action: Some(CommandPostAction::SendLinkedPrompt {
            connection_id,
            folder_id,
            conversation_id: conv.id,
            text: task_prompt,
            channel_id,
            sender_id: sender_id.to_string(),
            response_target: session_target,
            lang,
            trace_id: trace_id.map(|s| s.to_string()),
            route_key: route.route_key.clone(),
            registration_generation,
            previous_binding,
        }),
    }
}

// ── /sessions ──

pub async fn handle_sessions(
    db: &DatabaseConnection,
    channel_id: i32,
    sender_id: &str,
    target: &ChannelMessageTarget,
    route: &ConversationRoute,
    lang: Lang,
    prefix: &str,
) -> RichMessage {
    let conversation_id = match session_topic_access::authorized_conversation_id(
        db, channel_id, sender_id, target, route,
    )
    .await
    {
        Ok(Some(conversation_id)) => conversation_id,
        Ok(None) => {
            return RichMessage::info(session_topic_messages::no_session(lang, prefix));
        }
        Err(error) => {
            return RichMessage::error(format!(
                "{}{error}",
                i18n::failed_to_load_context_label(lang)
            ));
        }
    };
    let conversation = match conversation_service::get_by_id(db, conversation_id).await {
        Ok(conversation) => conversation,
        Err(_) => return RichMessage::info(session_topic_messages::no_session(lang, prefix)),
    };
    let title = conversation
        .title
        .as_deref()
        .unwrap_or(i18n::untitled(lang));
    let body = format!(
        "1. [{}] {} (#{}) [*]\n\n{}",
        conversation.agent_type,
        title,
        conversation.id,
        i18n::sessions_resume_hint(lang, prefix)
    );
    RichMessage::info(body).with_title(i18n::sessions_title(lang))
}

// ── /resume ──

#[allow(clippy::too_many_arguments)]
pub async fn handle_resume(
    db: &DatabaseConnection,
    args: &str,
    channel_id: i32,
    sender_id: &str,
    target: &ChannelMessageTarget,
    route: &ConversationRoute,
    manager: &ChatChannelManager,
    conn_mgr: &ConnectionManager,
    emitter: &EventEmitter,
    bridge: &Arc<Mutex<SessionBridge>>,
    lang: Lang,
    prefix: &str,
    data_dir: &Path,
    trace_id: Option<&str>,
    replace_existing: bool,
) -> RichMessage {
    if args.is_empty() {
        return handle_sessions(db, channel_id, sender_id, target, route, lang, prefix).await;
    }

    let conversation_id: i32 = match args.parse() {
        Ok(id) if id > 0 => id,
        _ => {
            return handle_sessions(db, channel_id, sender_id, target, route, lang, prefix).await;
        }
    };

    if target.is_telegram_general_topic() {
        return RichMessage::info(session_topic_messages::no_session(lang, prefix));
    }

    let conv = match conversation_service::get_by_id(db, conversation_id).await {
        Ok(c) => c,
        Err(_) => {
            return RichMessage::info(i18n::conversation_not_found(lang));
        }
    };
    let authorized = match route_authorizes_conversation(
        db,
        channel_id,
        sender_id,
        target,
        route,
        conversation_id,
    )
    .await
    {
        Ok(authorized) => authorized,
        Err(error) => {
            tracing::warn!(
                channel_id,
                conversation_id,
                error = %error,
                "[ChatChannel] resume authorization lookup failed"
            );
            false
        }
    };
    if !authorized {
        tracing::warn!(
            channel_id,
            conversation_id,
            route_key = %route.route_key,
            target_id = %route.target_id,
            "[ChatChannel] rejected resume for unbound conversation"
        );
        return RichMessage::info(i18n::conversation_not_found(lang));
    }
    let restoring_external = conv.external_id.is_some();

    if !replace_existing
        && session_topic::has_active_session(db, bridge, channel_id, &route.route_key).await
    {
        return RichMessage::info(session_topic_messages::active_session(lang, prefix));
    }

    let (connection_id, folder) = match session_runtime::spawn_for_conversation(
        db, &conv, channel_id, sender_id, target, conn_mgr, emitter, data_dir,
    )
    .await
    {
        Ok(started) => started,
        Err(error) => {
            return RichMessage::error(format!(
                "{}{error}",
                i18n::failed_to_start_agent_label(lang)
            ))
        }
    };

    if target.is_telegram_forum_topic() {
        if let Err(error) = session_topic::bind_target(
            db,
            target,
            conv.id,
            Some(connection_id.clone()),
            sender_id,
            conv.title.clone(),
        )
        .await
        {
            bridge.lock().await.remove(&connection_id);
            let _ = conn_mgr.cancel(db, &connection_id).await;
            return RichMessage::error(format!("Failed to bind Telegram topic: {error}"));
        }
        let title_context = ConversationTitleContext {
            conn: db,
            emitter,
            chat_channel_manager: manager,
        };
        conversation_title::sync_channels(&title_context, conv.id).await;
    } else {
        let _ = sender_context_service::update_session(
            db,
            channel_id,
            sender_id,
            Some(conv.id),
            Some(connection_id.clone()),
        )
        .await;
    }

    // Register only after transient route ownership is durable, then catch up
    // a SessionStarted event that may have arrived while the route was stored.
    {
        let session = ActiveSession {
            channel_id,
            sender_id: sender_id.to_string(),
            target: target.clone(),
            route_key: route.route_key.clone(),
            target_id: route.target_id.clone(),
            bind_on_start: true,
            conversation_id: conv.id,
            connection_id: connection_id.clone(),
            ownership: SessionOwnership::ChannelOwned,
            registration_generation: 0,
            restoring_external_id: conv.external_id.clone(),
            expected_external_id: conv.external_id.clone(),
            observed_session_id: None,
            agent_type: conv.agent_type,
            content_buffer: String::new(),
            tool_calls: Vec::new(),
            tool_call_inputs: std::collections::HashMap::new(),
            delegation_rendered: std::collections::HashSet::new(),
            last_flushed: Instant::now(),
            pending_prompt: None,
            recovery_prompt: None,
            pending_prompt_attempts: 0,
            trace_id: trace_id.map(|s| s.to_string()),
            permission_pending: None,
        };
        SessionBridge::register_serialized(bridge, connection_id.clone(), session).await;
    }
    session_event_subscriber::catch_up_session_start(bridge, manager, conn_mgr, db, &connection_id)
        .await;
    if restoring_external
        && session_event_subscriber::catch_up_session_load_failure(
            bridge,
            manager,
            conn_mgr,
            db,
            emitter,
            data_dir,
            &connection_id,
        )
        .await
    {
        return RichMessage::info("");
    }
    if bridge.lock().await.get(&connection_id).is_none() {
        return RichMessage::info("");
    }
    let _ = sender_context_service::update_folder(db, channel_id, sender_id, Some(conv.folder_id))
        .await;

    let title = conv.title.as_deref().unwrap_or(i18n::untitled(lang));
    RichMessage::info(format!(
        "[{}] #{} {} @ {}",
        conv.agent_type, conv.id, title, folder.name
    ))
    .with_title(i18n::session_resumed_title(lang))
}

// ── /cancel ──

pub async fn handle_cancel(
    db: &DatabaseConnection,
    channel_id: i32,
    sender_id: &str,
    target: &ChannelMessageTarget,
    route_key: &str,
    manager: &ChatChannelManager,
    conn_mgr: &ConnectionManager,
    bridge: &Arc<Mutex<SessionBridge>>,
    lang: Lang,
) -> RichMessage {
    let session_ref = match session_topic::command_session_ref(
        db, bridge, channel_id, sender_id, target, route_key,
    )
    .await
    {
        Ok(Some(reference)) => reference,
        Ok(None) => return RichMessage::info(i18n::no_active_session_to_cancel(lang)),
        Err(e) => {
            return RichMessage::error(format!("{}{e}", i18n::failed_to_load_context_label(lang)));
        }
    };

    // ACP cancel is turn-scoped. Keep the bridge, connection and durable
    // conversation binding so the next message continues this conversation.
    let _ = conn_mgr.cancel(db, &session_ref.connection_id).await;

    manager
        .typing_controller()
        .stop(manager, channel_id, route_key, &session_ref.connection_id)
        .await;

    RichMessage::info(i18n::task_cancelled_body(lang)).with_title(i18n::task_cancelled_title(lang))
}

// ── /approve, /deny ──

#[allow(clippy::too_many_arguments)]
pub async fn handle_permission_response(
    approve: bool,
    always: bool,
    db: &DatabaseConnection,
    channel_id: i32,
    sender_id: &str,
    target: &ChannelMessageTarget,
    route_key: &str,
    conn_mgr: &ConnectionManager,
    bridge: &Arc<Mutex<SessionBridge>>,
    lang: Lang,
) -> RichMessage {
    let session_ref = match session_topic::command_session_ref(
        db, bridge, channel_id, sender_id, target, route_key,
    )
    .await
    {
        Ok(Some(reference)) => reference,
        Ok(None) => return RichMessage::info(i18n::no_active_session(lang)),
        Err(e) => {
            return RichMessage::error(format!("{}{e}", i18n::failed_to_load_context_label(lang)));
        }
    };

    let pending = {
        let mut bridge_guard = bridge.lock().await;
        let session = match bridge_guard.get_mut(&session_ref.connection_id) {
            Some(s) => s,
            None => {
                if target.is_telegram_forum_topic() {
                    session_topic::clear_route_if_connection(
                        db,
                        channel_id,
                        sender_id,
                        target,
                        &session_ref.connection_id,
                    )
                    .await;
                }
                return RichMessage::info(i18n::no_active_session_found(lang));
            }
        };
        session.permission_pending.take()
    };

    let pending = match pending {
        Some(p) => p,
        None => {
            return RichMessage::info(i18n::no_pending_permission(lang));
        }
    };

    // Find the appropriate option_id
    let option_id = if approve {
        pending
            .options
            .iter()
            .find(|o| o.kind == "allow" || o.kind == "allowForSession")
            .or_else(|| pending.options.first())
            .map(|o| o.option_id.clone())
    } else {
        pending
            .options
            .iter()
            .find(|o| o.kind == "deny")
            .or_else(|| pending.options.last())
            .map(|o| o.option_id.clone())
    };

    let Some(option_id) = option_id else {
        return RichMessage::info(i18n::no_valid_permission_option(lang));
    };

    if let Err(e) = conn_mgr
        .respond_permission(&session_ref.connection_id, &pending.request_id, &option_id)
        .await
    {
        return RichMessage::error(format!(
            "{}{e}",
            i18n::failed_permission_response_label(lang)
        ));
    }

    // Update auto_approve if requested
    if always && approve {
        let _ = sender_context_service::update_auto_approve(db, channel_id, sender_id, true).await;
    }

    if approve {
        return RichMessage::info("");
    }

    RichMessage::info(i18n::denied_label(lang)).with_title(i18n::permission_response_title(lang))
}

// ── follow-up (non-command text) ──

pub async fn handle_followup(req: FollowupRequest<'_>) -> RichMessage {
    let active_session = {
        let guard = req.bridge.lock().await;
        guard
            .find_by_route(req.channel_id, &req.route.route_key)
            .map(|session| {
                (
                    session.connection_id.clone(),
                    session.conversation_id,
                    session.sender_id == req.sender_id
                        && session.target.matches_thread(req.target)
                        && session.target_id == req.route.target_id,
                )
            })
    };
    if let Some((connection_id, conversation_id, active_matches_target)) = active_session {
        let active_is_authorized = active_matches_target
            && route_authorizes_conversation(
                req.db,
                req.channel_id,
                req.sender_id,
                req.target,
                req.route,
                conversation_id,
            )
            .await
            .unwrap_or(false);
        if active_is_authorized {
            return send_followup_prompt(
                req.db,
                req.channel_id,
                req.sender_id,
                req.conn_mgr,
                req.bridge,
                &connection_id,
                req.text,
                req.lang,
                req.trace_id,
            )
            .await;
        }
        tracing::warn!(
            channel_id = req.channel_id,
            conversation_id,
            route_key = %req.route.route_key,
            "[ChatChannel] ignored active session without route authorization"
        );
    }

    let binding = match crate::db::service::conversation_binding_service::find_by_route(
        req.db,
        req.channel_id,
        &req.route.route_key,
    )
    .await
    {
        Ok(binding) => binding,
        Err(error) => {
            tracing::warn!(
                channel_id = req.channel_id,
                error = %error,
                "[ChatChannel] binding lookup failed"
            );
            return RichMessage::error(i18n::failed_to_load_context_label(req.lang));
        }
    };
    if let Some(binding) = binding.filter(|binding| binding.target_id == req.route.target_id) {
        tracing::info!(
            "[ChatChannel] follow-up resuming conversation={} channel={} sender={}",
            binding.conversation_id,
            req.channel_id,
            req.sender_id
        );
        return resume_conversation_for_followup(
            req.db,
            req.channel_id,
            req.sender_id,
            req.target,
            req.route,
            binding.conversation_id,
            req.text,
            req.manager,
            req.conn_mgr,
            req.emitter,
            req.bridge,
            req.data_dir,
            req.lang,
            req.trace_id,
        )
        .await;
    }

    tracing::info!(
        "[ChatChannel] follow-up ignored without active session channel={} sender={} prefix={}",
        req.channel_id,
        req.sender_id,
        req.prefix
    );
    RichMessage::info("")
}

#[allow(clippy::too_many_arguments)]
async fn send_followup_prompt(
    db: &DatabaseConnection,
    channel_id: i32,
    sender_id: &str,
    conn_mgr: &ConnectionManager,
    bridge: &Arc<Mutex<SessionBridge>>,
    connection_id: &str,
    text: &str,
    _lang: Lang,
    trace_id: Option<&str>,
) -> RichMessage {
    // Stamp the session with this follow-up's trace so the outbound reply
    // links back to the message that triggered it.
    if let Some(trace) = trace_id {
        let mut guard = bridge.lock().await;
        if let Some(session) = guard.get_mut(connection_id) {
            session.trace_id = Some(trace.to_string());
        }
    }

    // Send prompt to agent
    let blocks = vec![PromptInputBlock::Text {
        text: text.to_string(),
    }];

    tracing::info!(
        "[ChatChannel] follow-up enqueue start connection={} channel={} sender={} text_len={}",
        connection_id,
        channel_id,
        sender_id,
        text.chars().count()
    );

    if let Err(e) = conn_mgr.send_prompt(connection_id, blocks).await {
        // A turn is already in flight on this (shared) connection — another
        // client, or a previous prompt still running. This is transient: the
        // connection is alive, so do NOT tear down the bridge/session. Chat
        // channels only receive real assistant content, so this stays log-only.
        if matches!(e, crate::acp::error::AcpError::TurnInProgress) {
            tracing::info!(
                "[ChatChannel] follow-up enqueue blocked by in-flight turn \
                 connection={} channel={} sender={}",
                connection_id,
                channel_id,
                sender_id
            );
            return RichMessage::info("");
        }
        // Otherwise the connection may have died — clean up, but don't send a
        // canned channel reply. The next visible response must come from AI.
        bridge.lock().await.remove(connection_id);
        let _ = sender_context_service::clear_connection_if_matches(
            db,
            channel_id,
            sender_id,
            connection_id,
        )
        .await;
        tracing::warn!("[ChatChannel] failed to send follow-up prompt: {e}");
        return RichMessage::info("");
    }

    tracing::info!(
        "[ChatChannel] follow-up prompt enqueued connection={} channel={} sender={}",
        connection_id,
        channel_id,
        sender_id
    );
    RichMessage::info("")
}

/// A conversation can be resumed only through the durable route or topic
/// binding owned by the current message source.
async fn route_authorizes_conversation(
    db: &DatabaseConnection,
    channel_id: i32,
    sender_id: &str,
    target: &ChannelMessageTarget,
    route: &ConversationRoute,
    conversation_id: i32,
) -> Result<bool, crate::db::error::DbError> {
    Ok(
        session_topic_access::authorized_conversation_id(db, channel_id, sender_id, target, route)
            .await?
            == Some(conversation_id),
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn resume_conversation_for_followup(
    db: &DatabaseConnection,
    channel_id: i32,
    sender_id: &str,
    target: &ChannelMessageTarget,
    route: &ConversationRoute,
    conversation_id: i32,
    text: &str,
    manager: &ChatChannelManager,
    conn_mgr: &ConnectionManager,
    emitter: &EventEmitter,
    bridge: &Arc<Mutex<SessionBridge>>,
    data_dir: &Path,
    lang: Lang,
    trace_id: Option<&str>,
) -> RichMessage {
    let conv = match conversation_service::get_by_id(db, conversation_id).await {
        Ok(c) => c,
        Err(_) => return RichMessage::info(i18n::conversation_not_found(lang)),
    };
    if !route_authorizes_conversation(db, channel_id, sender_id, target, route, conversation_id)
        .await
        .unwrap_or(false)
    {
        tracing::warn!(
            channel_id,
            conversation_id,
            route_key = %route.route_key,
            target_id = %route.target_id,
            "[ChatChannel] rejected follow-up for unbound conversation"
        );
        return RichMessage::info(i18n::conversation_not_found(lang));
    }

    let folder = match folder_service::get_folder_by_id(db, conv.folder_id).await {
        Ok(Some(f)) => f,
        _ => return RichMessage::info(i18n::folder_not_found(lang)),
    };
    let recap = if conv.external_id.is_none() {
        get_conversation_context_primer_core(
            ContextPrimerSource {
                conn: db,
                manager: conn_mgr,
                chat_channel_manager: manager,
                emitter,
            },
            conv.id,
        )
        .await
        .ok()
        .map(|primer| primer.text)
    } else {
        None
    };
    let prompt_text = recap
        .as_deref()
        .map(|recap| format!("{recap}\n\n{text}"))
        .unwrap_or_else(|| text.to_string());
    let mut prompt =
        match channel_context::attach(db, channel_id, &route.target_id, &prompt_text).await {
            Ok(prompt) => prompt,
            Err(error) => {
                tracing::warn!(
                    channel_id,
                    error = %error,
                    "[ChatChannel] trusted channel context unavailable"
                );
                return RichMessage::error(i18n::failed_to_start_agent_label(lang));
            }
        };

    let expected_external_id = conv.external_id.clone();
    let had_external_id = expected_external_id.is_some();
    let live_connection = match conv.external_id.as_deref() {
        Some(external_id) => {
            conn_mgr
                .find_connection_by_external_id(external_id, conv.agent_type)
                .await
        }
        None => None,
    };
    let restoring_external = had_external_id && live_connection.is_none();

    tracing::info!(
        "[ChatChannel] follow-up resume target conversation={} channel={} sender={} \
         agent={:?} external_id_present={} live_connection_found={}",
        conversation_id,
        channel_id,
        sender_id,
        conv.agent_type,
        conv.external_id.is_some(),
        live_connection.is_some()
    );

    let ownership = if live_connection.is_some() {
        SessionOwnership::Borrowed
    } else {
        SessionOwnership::ChannelOwned
    };
    let (connection_id, send_now, recovered_with_recap, connection_expected_external_id) =
        match live_connection {
            Some(id) => (id, true, false, expected_external_id.clone()),
            None => {
                let runtime_env = match session_runtime::build_runtime_env(
                    db,
                    conv.agent_type,
                    conv.external_id.as_deref(),
                    data_dir,
                )
                .await
                {
                    Ok(runtime_env) => runtime_env,
                    Err(error) => {
                        tracing::warn!("[ChatChannel] failed to build runtime settings: {error}");
                        return RichMessage::error(i18n::failed_to_start_agent_label(lang));
                    }
                };
                let owner_label = session_runtime::owner_label(channel_id, sender_id, target);
                let id = match conn_mgr
                    .spawn_agent(
                        conv.agent_type,
                        Some(folder.path.clone()),
                        conv.external_id.clone(),
                        runtime_env,
                        owner_label,
                        emitter.clone(),
                        None,
                        BTreeMap::new(),
                    )
                    .await
                {
                    Ok(id) => id,
                    Err(e) => {
                        tracing::warn!("[ChatChannel] failed to resume conversation: {e}");
                        return RichMessage::error(i18n::failed_to_start_agent_label(lang));
                    }
                };
                if had_external_id && connection_recovery_failed(conn_mgr, &id).await {
                    let _ = conn_mgr.disconnect(&id).await;
                    if let Some(expected_external_id) = expected_external_id.as_deref() {
                        let cleared = match conversation_service::clear_external_id_if_matches(
                            db,
                            conv.id,
                            expected_external_id,
                        )
                        .await
                        {
                            Ok(cleared) => cleared,
                            Err(error) => {
                                tracing::warn!(
                                    conversation_id = conv.id,
                                    error = %error,
                                    "[ChatChannel] recovery fallback external id CAS failed"
                                );
                                false
                            }
                        };
                        tracing::info!(
                            conversation_id = conv.id,
                            cleared,
                            "[ChatChannel] recovery fallback external id CAS completed"
                        );
                        if !cleared {
                            tracing::info!(
                                conversation_id = conv.id,
                                "[ChatChannel] stale recovery fallback ignored"
                            );
                            return RichMessage::info("");
                        }
                    }
                    let recap = get_conversation_context_primer_core(
                        ContextPrimerSource {
                            conn: db,
                            manager: conn_mgr,
                            chat_channel_manager: manager,
                            emitter,
                        },
                        conv.id,
                    )
                    .await
                    .ok()
                    .map(|primer| primer.text);
                    let fallback_text = recap
                        .as_deref()
                        .map(|recap| format!("{recap}\n\n{text}"))
                        .unwrap_or_else(|| text.to_string());
                    prompt = match channel_context::attach(
                        db,
                        channel_id,
                        &route.target_id,
                        &fallback_text,
                    )
                    .await
                    {
                        Ok(prompt) => prompt,
                        Err(_) => {
                            return RichMessage::error(i18n::failed_to_start_agent_label(lang));
                        }
                    };
                    let runtime_env = match session_runtime::build_runtime_env(
                        db,
                        conv.agent_type,
                        None,
                        data_dir,
                    )
                    .await
                    {
                        Ok(runtime_env) => runtime_env,
                        Err(_) => {
                            return RichMessage::error(i18n::failed_to_start_agent_label(lang));
                        }
                    };
                    let fallback_id = match conn_mgr
                        .spawn_agent(
                            conv.agent_type,
                            Some(folder.path.clone()),
                            None,
                            runtime_env,
                            session_runtime::owner_label(channel_id, sender_id, target),
                            emitter.clone(),
                            None,
                            BTreeMap::new(),
                        )
                        .await
                    {
                        Ok(id) => id,
                        Err(_) => {
                            return RichMessage::error(i18n::failed_to_start_agent_label(lang));
                        }
                    };
                    (fallback_id, false, true, None)
                } else {
                    (id, had_external_id, false, expected_external_id.clone())
                }
            }
        };

    tracing::info!(
        "[ChatChannel] follow-up resume ready connection={} conversation={} \
         channel={} sender={} send_now={}",
        connection_id,
        conversation_id,
        channel_id,
        sender_id,
        send_now
    );

    let pending_prompt = (!send_now).then(|| prompt.clone());
    let recovery_prompt = (send_now && restoring_external).then(|| text.to_string());
    remember_sender_session(
        db,
        channel_id,
        sender_id,
        conv.id,
        conv.folder_id,
        conv.agent_type,
        connection_id.clone(),
    )
    .await;
    register_active_session(
        bridge,
        channel_id,
        sender_id,
        target,
        route,
        conv.id,
        connection_id.clone(),
        ownership,
        connection_expected_external_id,
        conv.agent_type,
        pending_prompt,
        recovery_prompt,
        trace_id.map(|s| s.to_string()),
    )
    .await;
    session_event_subscriber::catch_up_session_start(bridge, manager, conn_mgr, db, &connection_id)
        .await;
    if restoring_external
        && session_event_subscriber::catch_up_session_load_failure(
            bridge,
            manager,
            conn_mgr,
            db,
            emitter,
            data_dir,
            &connection_id,
        )
        .await
    {
        return RichMessage::info("");
    }

    if send_now {
        send_followup_prompt(
            db,
            channel_id,
            sender_id,
            conn_mgr,
            bridge,
            &connection_id,
            &prompt,
            lang,
            trace_id,
        )
        .await
    } else if recovered_with_recap {
        RichMessage::info(match lang {
            Lang::ZhCn | Lang::ZhTw => {
                "Agent 不支持恢复原会话，已自动以可见历史摘要继续处理本条消息。"
            }
            _ => {
                "The Agent could not restore the original session. This message is continuing automatically with a visible history recap."
            }
        })
    } else {
        RichMessage::info("")
    }
}

async fn connection_recovery_failed(conn_mgr: &ConnectionManager, connection_id: &str) -> bool {
    let Some(state) = conn_mgr.get_state(connection_id).await else {
        return true;
    };
    let recovery_failed = state.read().await.recovery_failed;
    recovery_failed
}

#[allow(clippy::too_many_arguments)]
async fn register_active_session(
    bridge: &Arc<Mutex<SessionBridge>>,
    channel_id: i32,
    sender_id: &str,
    target: &ChannelMessageTarget,
    route: &ConversationRoute,
    conversation_id: i32,
    connection_id: String,
    ownership: SessionOwnership,
    restoring_external_id: Option<String>,
    agent_type: AgentType,
    pending_prompt: Option<String>,
    recovery_prompt: Option<String>,
    trace_id: Option<String>,
) {
    let session = ActiveSession {
        channel_id,
        sender_id: sender_id.to_string(),
        target: target.clone(),
        route_key: route.route_key.clone(),
        target_id: route.target_id.clone(),
        bind_on_start: false,
        conversation_id,
        connection_id: connection_id.clone(),
        ownership,
        registration_generation: 0,
        restoring_external_id: restoring_external_id.clone(),
        expected_external_id: restoring_external_id.clone(),
        observed_session_id: None,
        agent_type,
        content_buffer: String::new(),
        tool_calls: Vec::new(),
        tool_call_inputs: std::collections::HashMap::new(),
        delegation_rendered: std::collections::HashSet::new(),
        last_flushed: Instant::now(),
        pending_prompt,
        recovery_prompt,
        pending_prompt_attempts: 0,
        trace_id,
        permission_pending: None,
    };
    SessionBridge::register_serialized(bridge, connection_id, session).await;
}

async fn remember_sender_session(
    db: &DatabaseConnection,
    channel_id: i32,
    sender_id: &str,
    conversation_id: i32,
    folder_id: i32,
    agent_type: AgentType,
    connection_id: String,
) {
    let _ = sender_context_service::update_session(
        db,
        channel_id,
        sender_id,
        Some(conversation_id),
        Some(connection_id),
    )
    .await;
    let _ = sender_context_service::update_folder(db, channel_id, sender_id, Some(folder_id)).await;
    let _ = sender_context_service::update_agent(
        db,
        channel_id,
        sender_id,
        Some(agent_type_to_string(agent_type)),
    )
    .await;
}

// ── Helpers ──

fn agent_type_to_string(at: AgentType) -> String {
    serde_json::to_value(at)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default()
}

fn parse_agent_type(name: &str) -> Option<AgentType> {
    parse_chat_agent(name)
}

fn resolve_agent_type(
    sender_agent: &Option<String>,
    channel_default: Option<AgentType>,
    folder_default: &Option<AgentType>,
) -> Option<AgentType> {
    if let Some(ref at_str) = sender_agent {
        if let Some(at) = parse_agent_type(at_str) {
            return is_chat_selectable_agent(at).then_some(at);
        }
    }
    channel_default
        .or_else(|| folder_default.as_ref().copied())
        .filter(|agent| is_chat_selectable_agent(*agent))
}

fn truncate_title(s: &str) -> String {
    if s.chars().count() <= 80 {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(77).collect();
        format!("{truncated}...")
    }
}

fn replacement_notice(lang: Lang, replacing_deleted: bool) -> RichMessage {
    if !replacing_deleted {
        return RichMessage::info("");
    }
    let body = match lang {
        Lang::ZhCn | Lang::ZhTw => "原绑定对话已被删除，已自动创建并绑定新对话。",
        _ => "The bound conversation was deleted. A new conversation was created and bound automatically.",
    };
    RichMessage::info(body)
}
