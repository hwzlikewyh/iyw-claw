use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use tokio::sync::Mutex;

use super::i18n::{self, Lang};
use super::manager::ChatChannelManager;
use super::natural_router;
use super::session_bridge::{ActiveSession, SessionBridge};
pub use super::session_dispatch::{
    handle_post_action, CommandMessageResult, CommandPostAction, SessionCommandMessage,
};
pub use super::session_picker::{handle_agent_picker, handle_callback, handle_folder_picker};
use super::session_runtime;
use super::session_topic;
use super::session_topic_messages;
use super::types::{ChannelMessageTarget, MessageLevel, RichMessage};
use crate::acp::manager::ConnectionManager;
use crate::acp::registry::all_acp_agents;
use crate::acp::types::{ConnectionStatus, PromptInputBlock};
use crate::db::entities::conversation;
use crate::db::service::{conversation_service, folder_service, sender_context_service};
use crate::models::agent::AgentType;
use crate::web::event_bridge::EventEmitter;

#[derive(Clone, Copy)]
pub struct FollowupRequest<'a> {
    pub db: &'a DatabaseConnection,
    pub text: &'a str,
    pub channel_id: i32,
    pub sender_id: &'a str,
    pub target: &'a ChannelMessageTarget,
    pub conn_mgr: &'a ConnectionManager,
    pub emitter: &'a EventEmitter,
    pub bridge: &'a Arc<Mutex<SessionBridge>>,
    pub data_dir: &'a Path,
    pub lang: Lang,
    pub prefix: &'a str,
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
    let agents = all_acp_agents();
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
    let agents = all_acp_agents();
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
    manager: &ChatChannelManager,
    conn_mgr: &ConnectionManager,
    emitter: &EventEmitter,
    bridge: &Arc<Mutex<SessionBridge>>,
    lang: Lang,
    prefix: &str,
    data_dir: &Path,
) -> CommandMessageResult {
    if task_description.is_empty() {
        return CommandMessageResult::current(
            RichMessage::info(i18n::task_usage(lang, prefix)),
            target,
        );
    }
    if session_topic::has_active_session(db, bridge, target).await {
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
        if let Some(title) = conv.title.as_deref() {
            manager.sync_conversation_title(db, conv.id, title).await;
        }
    }

    // 6. Register in bridge (prompt will be sent after SessionStarted event)
    {
        let session = ActiveSession {
            channel_id,
            sender_id: sender_id.to_string(),
            target: session_target.clone(),
            conversation_id: conv.id,
            connection_id: connection_id.clone(),
            agent_type,
            content_buffer: String::new(),
            tool_calls: Vec::new(),
            tool_call_inputs: std::collections::HashMap::new(),
            delegation_rendered: std::collections::HashSet::new(),
            last_flushed: Instant::now(),
            pending_prompt: None,
            permission_pending: None,
        };
        bridge.lock().await.register(connection_id.clone(), session);
    }

    // 7. Update sender context
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

    let started_message =
        RichMessage::info(format!("[{}] #{} @ {}", agent_type, conv.id, folder.name))
            .with_title(i18n::task_started_title(lang));
    let extra_responses = if target.is_telegram_general_topic() && session_target != *target {
        vec![(
            session_topic_messages::general_task_created(lang, agent_type, conv.id, &folder.name),
            target.clone(),
        )]
    } else {
        Vec::new()
    };
    CommandMessageResult {
        message: started_message,
        response_target: session_target.clone(),
        extra_responses,
        post_action: Some(CommandPostAction::SendLinkedPrompt {
            connection_id,
            folder_id,
            conversation_id: conv.id,
            text: task_description.to_string(),
            channel_id,
            sender_id: sender_id.to_string(),
            response_target: session_target,
            lang,
        }),
    }
}

// ── /sessions ──

pub async fn handle_sessions(
    db: &DatabaseConnection,
    channel_id: i32,
    sender_id: &str,
    target: &ChannelMessageTarget,
    lang: Lang,
    prefix: &str,
) -> RichMessage {
    let ctx = match sender_context_service::get_or_create(db, channel_id, sender_id).await {
        Ok(c) => c,
        Err(e) => {
            return RichMessage::error(format!("{}{e}", i18n::failed_to_load_context_label(lang)));
        }
    };

    let topic_conversation_id = if target.is_telegram_forum_topic() {
        crate::db::service::thread_binding_service::get_by_target(db, target)
            .await
            .ok()
            .flatten()
            .map(|binding| binding.conversation_id)
    } else {
        None
    };

    let folder_id = match ctx.current_folder_id {
        Some(id) => id,
        None => {
            return RichMessage::info(i18n::no_folder_selected(lang, prefix));
        }
    };

    let folder = match folder_service::get_folder_by_id(db, folder_id).await {
        Ok(Some(f)) => f,
        _ => {
            return RichMessage::info(i18n::folder_not_found(lang));
        }
    };

    let convs = match conversation_service::list_by_folder(
        db,
        folder_id,
        None,
        None,
        None,
        Some("in_progress".to_string()),
    )
    .await
    {
        Ok(c) => c,
        Err(e) => {
            return RichMessage::error(format!("{}{e}", i18n::failed_to_list_sessions_label(lang)));
        }
    };

    if convs.is_empty() {
        return RichMessage::info(i18n::no_active_sessions_in_folder(lang)).with_title(format!(
            "{} - {}",
            i18n::sessions_title(lang),
            folder.name
        ));
    }

    let mut body = String::new();
    for (i, c) in convs.iter().take(10).enumerate() {
        let title = c.title.as_deref().unwrap_or("(untitled)");
        let current = topic_conversation_id
            .or(ctx.current_conversation_id)
            .is_some_and(|id| id == c.id);
        let marker = if current { " [*]" } else { "" };
        body.push_str(&format!(
            "{}. [{}] {} (#{}){}  \n",
            i + 1,
            c.agent_type,
            title,
            c.id,
            marker,
        ));
    }

    body.push_str(&format!("\n{}", i18n::sessions_resume_hint(lang, prefix)));

    RichMessage::info(body.trim_end()).with_title(format!(
        "{} - {}",
        i18n::sessions_title(lang),
        folder.name
    ))
}

// ── /resume ──

#[allow(clippy::too_many_arguments)]
pub async fn handle_resume(
    db: &DatabaseConnection,
    args: &str,
    channel_id: i32,
    sender_id: &str,
    target: &ChannelMessageTarget,
    manager: &ChatChannelManager,
    conn_mgr: &ConnectionManager,
    emitter: &EventEmitter,
    bridge: &Arc<Mutex<SessionBridge>>,
    lang: Lang,
    prefix: &str,
    data_dir: &Path,
) -> RichMessage {
    if args.is_empty() {
        return list_recent_sessions(db, lang, prefix).await;
    }

    let conversation_id: i32 = match args.parse() {
        Ok(id) => id,
        Err(_) => {
            return list_recent_sessions(db, lang, prefix).await;
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

    if session_topic::has_active_session(db, bridge, target).await {
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

    // Register in bridge (no pending prompt for resume)
    {
        let session = ActiveSession {
            channel_id,
            sender_id: sender_id.to_string(),
            target: target.clone(),
            conversation_id: conv.id,
            connection_id: connection_id.clone(),
            agent_type: conv.agent_type,
            content_buffer: String::new(),
            tool_calls: Vec::new(),
            tool_call_inputs: std::collections::HashMap::new(),
            delegation_rendered: std::collections::HashSet::new(),
            last_flushed: Instant::now(),
            pending_prompt: None,
            permission_pending: None,
        };
        bridge.lock().await.register(connection_id.clone(), session);
    }

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
        if let Some(title) = conv.title.as_deref() {
            manager.sync_conversation_title(db, conv.id, title).await;
        }
    } else {
        let _ = sender_context_service::update_session(
            db,
            channel_id,
            sender_id,
            Some(conv.id),
            Some(connection_id),
        )
        .await;
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
    conn_mgr: &ConnectionManager,
    bridge: &Arc<Mutex<SessionBridge>>,
    lang: Lang,
) -> RichMessage {
    let session_ref =
        match session_topic::command_session_ref(db, bridge, channel_id, sender_id, target).await {
            Ok(Some(reference)) => reference,
            Ok(None) => return RichMessage::info(i18n::no_active_session_to_cancel(lang)),
            Err(e) => {
                return RichMessage::error(format!(
                    "{}{e}",
                    i18n::failed_to_load_context_label(lang)
                ));
            }
        };

    // Cancel the ACP connection (also CAS-updates the row to Cancelled and
    // emits ConversationStatusChanged when the row is still InProgress).
    let _ = conn_mgr.cancel(db, &session_ref.connection_id).await;

    // Remove from bridge
    bridge.lock().await.remove(&session_ref.connection_id);

    // Update conversation status
    if let Some(conv_id) = session_ref.conversation_id {
        let _ = conversation_service::update_status(
            db,
            conv_id,
            conversation::ConversationStatus::Cancelled,
        )
        .await;
    }

    // Clear session from context
    session_topic::clear_route(db, channel_id, sender_id, target).await;

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
    conn_mgr: &ConnectionManager,
    bridge: &Arc<Mutex<SessionBridge>>,
    lang: Lang,
) -> RichMessage {
    let session_ref =
        match session_topic::command_session_ref(db, bridge, channel_id, sender_id, target).await {
            Ok(Some(reference)) => reference,
            Ok(None) => return RichMessage::info(i18n::no_active_session(lang)),
            Err(e) => {
                return RichMessage::error(format!(
                    "{}{e}",
                    i18n::failed_to_load_context_label(lang)
                ));
            }
        };

    let pending = {
        let mut bridge_guard = bridge.lock().await;
        let session = match bridge_guard.get_mut(&session_ref.connection_id) {
            Some(s) => s,
            None => {
                if target.is_telegram_forum_topic() {
                    session_topic::clear_route(db, channel_id, sender_id, target).await;
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
    if req.target.is_telegram_forum_topic() {
        return session_topic::handle_followup(req).await;
    }

    let ctx =
        match sender_context_service::get_or_create(req.db, req.channel_id, req.sender_id).await {
            Ok(c) => c,
            Err(e) => {
                return RichMessage::error(format!(
                    "{}{e}",
                    i18n::failed_to_load_context_label(req.lang)
                ));
            }
        };

    tracing::info!(
        "[ChatChannel] follow-up route channel={} sender={} has_connection={} \
         has_conversation={}",
        req.channel_id,
        req.sender_id,
        ctx.current_connection_id.is_some(),
        ctx.current_conversation_id.is_some()
    );

    if let Some(connection_id) = ctx.current_connection_id.clone() {
        let bridge_has_session = {
            let bridge_guard = req.bridge.lock().await;
            bridge_guard.get(&connection_id).is_some()
        };

        if bridge_has_session {
            tracing::info!(
                "[ChatChannel] follow-up using active bridge connection={} channel={} sender={}",
                connection_id,
                req.channel_id,
                req.sender_id
            );
            return send_followup_prompt(
                req.db,
                req.channel_id,
                req.sender_id,
                req.conn_mgr,
                req.bridge,
                &connection_id,
                req.text,
                req.lang,
            )
            .await;
        }

        if let Some(conversation_id) = ctx.current_conversation_id {
            tracing::info!(
                "[ChatChannel] follow-up trying bridge restore connection={} \
                 conversation={} channel={} sender={}",
                connection_id,
                conversation_id,
                req.channel_id,
                req.sender_id
            );
            if restore_bridge_session_from_live_connection(
                req.db,
                req.channel_id,
                req.sender_id,
                req.target,
                conversation_id,
                &connection_id,
                req.conn_mgr,
                req.bridge,
            )
            .await
            {
                tracing::info!(
                    "[ChatChannel] follow-up restored bridge connection={} \
                     conversation={} channel={} sender={}",
                    connection_id,
                    conversation_id,
                    req.channel_id,
                    req.sender_id
                );
                return send_followup_prompt(
                    req.db,
                    req.channel_id,
                    req.sender_id,
                    req.conn_mgr,
                    req.bridge,
                    &connection_id,
                    req.text,
                    req.lang,
                )
                .await;
            }
        }
    }

    if let Some(conversation_id) = ctx.current_conversation_id {
        tracing::info!(
            "[ChatChannel] follow-up resuming conversation={} channel={} sender={}",
            conversation_id,
            req.channel_id,
            req.sender_id
        );
        return resume_conversation_for_followup(
            req.db,
            req.channel_id,
            req.sender_id,
            req.target,
            conversation_id,
            req.text,
            req.conn_mgr,
            req.emitter,
            req.bridge,
            req.data_dir,
            req.lang,
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
) -> RichMessage {
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
        let _ = sender_context_service::clear_session(db, channel_id, sender_id).await;
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

#[allow(clippy::too_many_arguments)]
async fn restore_bridge_session_from_live_connection(
    db: &DatabaseConnection,
    channel_id: i32,
    sender_id: &str,
    target: &ChannelMessageTarget,
    conversation_id: i32,
    connection_id: &str,
    conn_mgr: &ConnectionManager,
    bridge: &Arc<Mutex<SessionBridge>>,
) -> bool {
    let Some(state) = conn_mgr.get_state(connection_id).await else {
        return false;
    };
    let state = state.read().await;
    if matches!(
        state.status,
        ConnectionStatus::Disconnected | ConnectionStatus::Error
    ) {
        return false;
    }
    let agent_type = state.agent_type;
    drop(state);

    let Ok(conv) = conversation_service::get_by_id(db, conversation_id).await else {
        return false;
    };
    if conv.agent_type != agent_type {
        return false;
    }

    register_active_session(
        bridge,
        channel_id,
        sender_id,
        target,
        conv.id,
        connection_id.to_string(),
        conv.agent_type,
        None,
    )
    .await;
    remember_sender_session(
        db,
        channel_id,
        sender_id,
        conv.id,
        conv.folder_id,
        conv.agent_type,
        connection_id.to_string(),
    )
    .await;
    true
}

#[allow(clippy::too_many_arguments)]
async fn resume_conversation_for_followup(
    db: &DatabaseConnection,
    channel_id: i32,
    sender_id: &str,
    target: &ChannelMessageTarget,
    conversation_id: i32,
    text: &str,
    conn_mgr: &ConnectionManager,
    emitter: &EventEmitter,
    bridge: &Arc<Mutex<SessionBridge>>,
    data_dir: &Path,
    lang: Lang,
) -> RichMessage {
    let conv = match conversation_service::get_by_id(db, conversation_id).await {
        Ok(c) => c,
        Err(_) => return RichMessage::info(i18n::conversation_not_found(lang)),
    };

    let folder = match folder_service::get_folder_by_id(db, conv.folder_id).await {
        Ok(Some(f)) => f,
        _ => return RichMessage::info(i18n::folder_not_found(lang)),
    };

    let live_connection = match conv.external_id.as_deref() {
        Some(external_id) => {
            conn_mgr
                .find_connection_by_external_id(external_id, conv.agent_type)
                .await
        }
        None => None,
    };

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

    let (connection_id, send_now) = match live_connection {
        Some(id) => (id, true),
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
            (id, conv.external_id.is_some())
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

    let pending_prompt = (!send_now).then(|| text.to_string());
    register_active_session(
        bridge,
        channel_id,
        sender_id,
        target,
        conv.id,
        connection_id.clone(),
        conv.agent_type,
        pending_prompt,
    )
    .await;
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

    if send_now {
        send_followup_prompt(
            db,
            channel_id,
            sender_id,
            conn_mgr,
            bridge,
            &connection_id,
            text,
            lang,
        )
        .await
    } else {
        RichMessage::info("")
    }
}

#[allow(clippy::too_many_arguments)]
async fn register_active_session(
    bridge: &Arc<Mutex<SessionBridge>>,
    channel_id: i32,
    sender_id: &str,
    target: &ChannelMessageTarget,
    conversation_id: i32,
    connection_id: String,
    agent_type: AgentType,
    pending_prompt: Option<String>,
) {
    let session = ActiveSession {
        channel_id,
        sender_id: sender_id.to_string(),
        target: target.clone(),
        conversation_id,
        connection_id: connection_id.clone(),
        agent_type,
        content_buffer: String::new(),
        tool_calls: Vec::new(),
        tool_call_inputs: std::collections::HashMap::new(),
        delegation_rendered: std::collections::HashSet::new(),
        last_flushed: Instant::now(),
        pending_prompt,
        permission_pending: None,
    };
    bridge.lock().await.register(connection_id, session);
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

// ── /resume (list recent) ──

async fn list_recent_sessions(db: &DatabaseConnection, lang: Lang, prefix: &str) -> RichMessage {
    let recent = match conversation::Entity::find()
        .filter(conversation::Column::DeletedAt.is_null())
        .order_by_desc(conversation::Column::CreatedAt)
        .limit(10)
        .all(db)
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            return RichMessage {
                title: Some(i18n::query_failed_title(lang).to_string()),
                body: e.to_string(),
                fields: Vec::new(),
                level: MessageLevel::Error,
            };
        }
    };

    if recent.is_empty() {
        return RichMessage::info(i18n::no_conversations_found(lang))
            .with_title(i18n::recent_conversations_title(lang));
    }

    let mut body = String::new();
    for conv in &recent {
        let title = conv.title.as_deref().unwrap_or(i18n::untitled(lang));
        let agent = &conv.agent_type;
        let time = conv.created_at.format("%m-%d %H:%M");
        body.push_str(&format!("#{} [{}] {} ({})\n", conv.id, agent, title, time,));
    }

    body.push_str(&format!("\n{}", i18n::recent_resume_hint(lang, prefix)));

    RichMessage::info(body.trim_end()).with_title(i18n::recent_conversations_title(lang))
}

// ── Helpers ──

fn agent_type_to_string(at: AgentType) -> String {
    serde_json::to_value(at)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default()
}

fn parse_agent_type(name: &str) -> Option<AgentType> {
    let normalized = name.to_lowercase().replace([' ', '-'], "_");
    serde_json::from_value(serde_json::Value::String(normalized)).ok()
}

fn resolve_agent_type(
    sender_agent: &Option<String>,
    channel_default: Option<AgentType>,
    folder_default: &Option<AgentType>,
) -> Option<AgentType> {
    if let Some(ref at_str) = sender_agent {
        if let Some(at) = parse_agent_type(at_str) {
            return Some(at);
        }
    }
    channel_default.or_else(|| folder_default.as_ref().copied())
}

fn truncate_title(s: &str) -> String {
    if s.chars().count() <= 80 {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(77).collect();
        format!("{truncated}...")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acp::connection::ConnectionCommand;
    use crate::acp::types::PermissionOptionInfo;
    use crate::chat_channel::session_bridge::{ActiveSession, PendingPermission, SessionBridge};
    use crate::db::service::sender_context_service;
    use crate::db::test_helpers;
    use crate::web::event_bridge::EventEmitter;
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;
    use std::time::Instant;
    use tokio::sync::Mutex;

    #[test]
    fn resolve_agent_type_prefers_sender_then_channel_then_folder() {
        let sender = Some("gemini".to_string());
        assert_eq!(
            resolve_agent_type(&sender, Some(AgentType::Codex), &Some(AgentType::ClaudeCode)),
            Some(AgentType::Gemini)
        );
        assert_eq!(
            resolve_agent_type(&None, Some(AgentType::Codex), &Some(AgentType::ClaudeCode)),
            Some(AgentType::Codex)
        );
        assert_eq!(
            resolve_agent_type(&None, None, &Some(AgentType::ClaudeCode)),
            Some(AgentType::ClaudeCode)
        );
        assert_eq!(resolve_agent_type(&None, None, &None), None);
    }

    #[tokio::test]
    async fn approving_permission_responds_to_agent_without_channel_reply() {
        let db = test_helpers::fresh_in_memory_db().await;
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
        let conn_mgr = ConnectionManager::new();
        let mut cmd_rx = conn_mgr
            .insert_test_connection_live("conn-approve", AgentType::Codex, None, EventEmitter::Noop)
            .await;
        let bridge = Arc::new(Mutex::new(SessionBridge::new()));
        bridge.lock().await.register(
            "conn-approve".to_string(),
            ActiveSession {
                channel_id: channel.id,
                sender_id: "u1".to_string(),
                target: ChannelMessageTarget::channel(channel.id),
                conversation_id: 1,
                connection_id: "conn-approve".to_string(),
                agent_type: AgentType::Codex,
                content_buffer: String::new(),
                tool_calls: Vec::new(),
                tool_call_inputs: HashMap::new(),
                delegation_rendered: HashSet::new(),
                last_flushed: Instant::now(),
                pending_prompt: None,
                permission_pending: Some(PendingPermission {
                    request_id: "perm-1".to_string(),
                    tool_description: "Bash: cargo test".to_string(),
                    options: vec![
                        PermissionOptionInfo {
                            option_id: "allow".to_string(),
                            name: "Allow".to_string(),
                            kind: "allow".to_string(),
                        },
                        PermissionOptionInfo {
                            option_id: "deny".to_string(),
                            name: "Deny".to_string(),
                            kind: "deny".to_string(),
                        },
                    ],
                    sent_message_id: None,
                }),
            },
        );
        sender_context_service::update_session(
            &db.conn,
            channel.id,
            "u1",
            Some(1),
            Some("conn-approve".to_string()),
        )
        .await
        .unwrap();

        let msg = handle_permission_response(
            true,
            false,
            &db.conn,
            channel.id,
            "u1",
            &ChannelMessageTarget::channel(channel.id),
            &conn_mgr,
            &bridge,
            Lang::ZhCn,
        )
        .await;

        assert!(msg.title.is_none());
        assert!(msg.body.is_empty());
        assert!(msg.fields.is_empty());
        let command = cmd_rx.try_recv().expect("approval command should be sent");
        assert!(
            matches!(
                command,
                ConnectionCommand::RespondPermission {
                    request_id,
                    option_id,
                } if request_id == "perm-1" && option_id == "allow"
            ),
            "unexpected permission command"
        );
    }

    #[tokio::test]
    async fn followup_busy_connection_stays_silent() {
        let db = test_helpers::fresh_in_memory_db().await;
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
        let folder_id = test_helpers::seed_folder(&db, "D:/projects/iyw-claw").await;
        let conversation_id =
            test_helpers::seed_conversation(&db, folder_id, AgentType::Codex).await;
        let conn_mgr = ConnectionManager::new();
        let _cmd_rx = conn_mgr
            .insert_test_connection_live("conn-busy", AgentType::Codex, None, EventEmitter::Noop)
            .await;
        conn_mgr
            .get_state("conn-busy")
            .await
            .unwrap()
            .write()
            .await
            .turn_in_flight = true;
        let bridge = Arc::new(Mutex::new(SessionBridge::new()));
        bridge.lock().await.register(
            "conn-busy".to_string(),
            ActiveSession {
                channel_id: channel.id,
                sender_id: "u1".to_string(),
                target: ChannelMessageTarget::channel(channel.id),
                conversation_id,
                connection_id: "conn-busy".to_string(),
                agent_type: AgentType::Codex,
                content_buffer: String::new(),
                tool_calls: Vec::new(),
                tool_call_inputs: HashMap::new(),
                delegation_rendered: HashSet::new(),
                last_flushed: Instant::now(),
                pending_prompt: None,
                permission_pending: None,
            },
        );
        sender_context_service::update_session(
            &db.conn,
            channel.id,
            "u1",
            Some(conversation_id),
            Some("conn-busy".to_string()),
        )
        .await
        .unwrap();

        let msg = handle_followup(FollowupRequest {
            db: &db.conn,
            text: "你好",
            channel_id: channel.id,
            sender_id: "u1",
            target: &ChannelMessageTarget::channel(channel.id),
            conn_mgr: &conn_mgr,
            emitter: &EventEmitter::Noop,
            bridge: &bridge,
            data_dir: Path::new("D:/tmp/iyw-claw-chat-test"),
            lang: Lang::ZhCn,
            prefix: "/",
        })
        .await;

        assert!(msg.is_silent(), "busy follow-up must not send canned text");
        assert!(
            bridge.lock().await.get("conn-busy").is_some(),
            "busy live connection should remain bridged"
        );
    }

    #[tokio::test]
    async fn followup_restores_missing_bridge_for_live_context_connection() {
        let db = test_helpers::fresh_in_memory_db().await;
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
        let folder_id = test_helpers::seed_folder(&db, "D:/projects/iyw-claw").await;
        let conversation_id =
            test_helpers::seed_conversation(&db, folder_id, AgentType::Codex).await;
        let conn_mgr = ConnectionManager::new();
        let mut cmd_rx = conn_mgr
            .insert_test_connection_live("conn-live", AgentType::Codex, None, EventEmitter::Noop)
            .await;
        let bridge = Arc::new(Mutex::new(SessionBridge::new()));
        sender_context_service::update_session(
            &db.conn,
            channel.id,
            "u1",
            Some(conversation_id),
            Some("conn-live".to_string()),
        )
        .await
        .unwrap();

        let msg = handle_followup(FollowupRequest {
            db: &db.conn,
            text: "你好",
            channel_id: channel.id,
            sender_id: "u1",
            target: &ChannelMessageTarget::channel(channel.id),
            conn_mgr: &conn_mgr,
            emitter: &EventEmitter::Noop,
            bridge: &bridge,
            data_dir: Path::new("D:/tmp/iyw-claw-chat-test"),
            lang: Lang::ZhCn,
            prefix: "/",
        })
        .await;

        assert!(msg.is_silent(), "follow-up ack must stay silent");
        assert!(
            bridge.lock().await.get("conn-live").is_some(),
            "bridge should be restored for the live connection"
        );
        let command = cmd_rx.try_recv().expect("prompt should be sent");
        assert!(
            matches!(
                command,
                ConnectionCommand::Prompt { blocks, .. }
                    if matches!(blocks.as_slice(), [PromptInputBlock::Text { text }] if text == "你好")
            ),
            "unexpected prompt command"
        );
        let rows = conversation_service::list_all(&db.conn, None, None, None, None, None, true)
            .await
            .unwrap();
        assert_eq!(
            rows.len(),
            1,
            "follow-up must not create a new conversation"
        );
    }

    #[tokio::test]
    async fn followup_recovers_live_connection_by_existing_conversation_external_id() {
        let db = test_helpers::fresh_in_memory_db().await;
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
        let folder_id = test_helpers::seed_folder(&db, "D:/projects/iyw-claw").await;
        let conversation_id =
            test_helpers::seed_conversation(&db, folder_id, AgentType::Codex).await;
        conversation_service::update_external_id(&db.conn, conversation_id, "ext-1".to_string())
            .await
            .unwrap();
        let conn_mgr = ConnectionManager::new();
        let mut cmd_rx = conn_mgr
            .insert_test_connection_live("conn-ext", AgentType::Codex, None, EventEmitter::Noop)
            .await;
        conn_mgr
            .get_state("conn-ext")
            .await
            .unwrap()
            .write()
            .await
            .external_id = Some("ext-1".to_string());
        let bridge = Arc::new(Mutex::new(SessionBridge::new()));
        sender_context_service::update_session(
            &db.conn,
            channel.id,
            "u1",
            Some(conversation_id),
            None,
        )
        .await
        .unwrap();

        let msg = handle_followup(FollowupRequest {
            db: &db.conn,
            text: "继续",
            channel_id: channel.id,
            sender_id: "u1",
            target: &ChannelMessageTarget::channel(channel.id),
            conn_mgr: &conn_mgr,
            emitter: &EventEmitter::Noop,
            bridge: &bridge,
            data_dir: Path::new("D:/tmp/iyw-claw-chat-test"),
            lang: Lang::ZhCn,
            prefix: "/",
        })
        .await;

        assert!(msg.is_silent(), "follow-up ack must stay silent");
        assert!(
            bridge.lock().await.get("conn-ext").is_some(),
            "bridge should be restored from the conversation external id"
        );
        let ctx = sender_context_service::get_or_create(&db.conn, channel.id, "u1")
            .await
            .unwrap();
        assert_eq!(ctx.current_connection_id.as_deref(), Some("conn-ext"));
        let command = cmd_rx.try_recv().expect("prompt should be sent");
        assert!(
            matches!(
                command,
                ConnectionCommand::Prompt { blocks, .. }
                    if matches!(blocks.as_slice(), [PromptInputBlock::Text { text }] if text == "继续")
            ),
            "unexpected prompt command"
        );
        let rows = conversation_service::list_all(&db.conn, None, None, None, None, None, true)
            .await
            .unwrap();
        assert_eq!(
            rows.len(),
            1,
            "follow-up must not create a new conversation"
        );
    }
}
