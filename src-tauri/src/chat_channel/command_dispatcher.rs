use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use sea_orm::DatabaseConnection;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

use super::command_handlers;
use super::command_response::{send_dispatch_message, DispatchResponse};
use super::i18n::{self, Lang};
use super::manager::ChatChannelManager;
use super::natural_router::{self, NaturalRouteDecision};
use super::session_bridge::SessionBridge;
use super::session_commands;
use super::types::{ChannelMessageTarget, IncomingCommand, RichMessage};
use crate::acp::manager::ConnectionManager;
use crate::db::service::{
    app_metadata_service, chat_channel_message_log_service, sender_context_service,
};
use crate::web::event_bridge::EventEmitter;

const COMMAND_PREFIX_KEY: &str = "chat_command_prefix";
const DEFAULT_COMMAND_PREFIX: &str = "/";
const MESSAGE_LANGUAGE_KEY: &str = "chat_message_language";
/// How often to refresh cached config from DB.
const CONFIG_CACHE_TTL_SECS: u64 = 30;

struct CommandConfigCache {
    prefix: String,
    lang: Lang,
    last_refresh: Instant,
}

impl CommandConfigCache {
    fn new() -> Self {
        Self {
            prefix: DEFAULT_COMMAND_PREFIX.to_string(),
            lang: Lang::default(),
            // Force refresh on first use
            last_refresh: Instant::now() - Duration::from_secs(CONFIG_CACHE_TTL_SECS + 1),
        }
    }

    async fn refresh_if_needed(&mut self, db: &DatabaseConnection) {
        if self.last_refresh.elapsed() < Duration::from_secs(CONFIG_CACHE_TTL_SECS) {
            return;
        }

        if let Ok(Some(val)) = app_metadata_service::get_value(db, COMMAND_PREFIX_KEY).await {
            self.prefix = val;
        }
        if let Ok(Some(val)) = app_metadata_service::get_value(db, MESSAGE_LANGUAGE_KEY).await {
            self.lang = Lang::from_str_lossy(&val);
        }

        self.last_refresh = Instant::now();
    }
}

pub fn spawn_command_dispatcher(
    mut command_rx: mpsc::Receiver<IncomingCommand>,
    manager: ChatChannelManager,
    db_conn: DatabaseConnection,
    data_dir: PathBuf,
    conn_mgr: ConnectionManager,
    emitter: EventEmitter,
    bridge: Arc<Mutex<SessionBridge>>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut config = CommandConfigCache::new();

        while let Some(cmd) = command_rx.recv().await {
            let text = cmd.command_text.trim();
            tracing::info!(
                "[ChatChannel] received command from channel={} sender={}: {:?}",
                cmd.channel_id,
                cmd.sender_id,
                text
            );

            // Log inbound command
            let _ = chat_channel_message_log_service::create_log(
                &db_conn,
                cmd.channel_id,
                "inbound",
                "command_query",
                text,
                "sent",
                None,
            )
            .await;

            config.refresh_if_needed(&db_conn).await;

            let mut response = dispatch_command(
                text,
                &config.prefix,
                &db_conn,
                &manager,
                &conn_mgr,
                &emitter,
                &bridge,
                &data_dir,
                cmd.channel_id,
                &cmd.sender_id,
                cmd.sender_name.as_deref(),
                &cmd.target,
                cmd.callback_data.as_deref(),
                config.lang,
            )
            .await;

            for (message, target) in response.take_messages() {
                send_dispatch_message(&db_conn, &manager, cmd.channel_id, text, message, target)
                    .await;
            }

            if let Some(action) = response.post_action.take() {
                if let Some((message, target)) =
                    session_commands::handle_post_action(action, &db_conn, &conn_mgr, &bridge).await
                {
                    let mut response = DispatchResponse::current(message, &target);
                    for (message, target) in response.take_messages() {
                        send_dispatch_message(
                            &db_conn,
                            &manager,
                            cmd.channel_id,
                            text,
                            message,
                            target,
                        )
                        .await;
                    }
                }
            }
        }
    })
}

#[allow(clippy::too_many_arguments)]
async fn dispatch_command(
    text: &str,
    prefix: &str,
    db: &DatabaseConnection,
    manager: &ChatChannelManager,
    conn_mgr: &ConnectionManager,
    emitter: &EventEmitter,
    bridge: &Arc<Mutex<SessionBridge>>,
    data_dir: &Path,
    channel_id: i32,
    sender_id: &str,
    sender_name: Option<&str>,
    target: &ChannelMessageTarget,
    callback_data: Option<&str>,
    lang: Lang,
) -> DispatchResponse {
    if let Some(data) = callback_data {
        return DispatchResponse::current(
            session_commands::handle_callback(db, data, channel_id, sender_id, lang, prefix).await,
            target,
        );
    }

    // Strip prefix; if text doesn't start with it, try as follow-up
    let without_prefix = match text.strip_prefix(prefix) {
        Some(rest) => rest,
        None => {
            if target.is_telegram_general_topic() {
                return DispatchResponse::none(target);
            }
            if target.is_telegram_forum_topic() {
                return DispatchResponse::current(
                    session_commands::handle_followup(session_commands::FollowupRequest {
                        db,
                        text,
                        channel_id,
                        sender_id,
                        target,
                        conn_mgr,
                        emitter,
                        bridge,
                        data_dir,
                        lang,
                        prefix,
                    })
                    .await,
                    target,
                );
            }
            return dispatch_natural_message(
                text, prefix, db, manager, conn_mgr, emitter, bridge, data_dir, channel_id,
                sender_id, sender_name, target, lang,
            )
            .await;
        }
    };

    let parts: Vec<&str> = without_prefix.splitn(2, ' ').collect();
    let command = parts[0].to_lowercase();
    let args = parts.get(1).map(|s| s.trim()).unwrap_or("");

    match command.as_str() {
        // Existing commands
        "search" => {
            if args.is_empty() {
                DispatchResponse::current(
                    RichMessage::info(i18n::search_usage(lang, prefix))
                        .with_title(i18n::invalid_args_title(lang)),
                    target,
                )
            } else {
                DispatchResponse::current(
                    command_handlers::handle_search(db, args, lang).await,
                    target,
                )
            }
        }
        "today" => {
            DispatchResponse::current(command_handlers::handle_today(db, lang).await, target)
        }
        "status" => {
            DispatchResponse::current(command_handlers::handle_status(manager, lang).await, target)
        }
        "help" | "start" => {
            DispatchResponse::current(command_handlers::handle_help(prefix, lang), target)
        }

        // Session commands
        "folder" => {
            if args.is_empty() {
                DispatchResponse::from_session_message(
                    session_commands::handle_folder_picker(db, channel_id, sender_id, lang, prefix)
                        .await,
                    target,
                )
            } else {
                DispatchResponse::current(
                    session_commands::handle_folder(db, args, channel_id, sender_id, lang, prefix)
                        .await,
                    target,
                )
            }
        }
        "agent" => {
            if args.is_empty() {
                DispatchResponse::from_session_message(
                    session_commands::handle_agent_picker(db, channel_id, sender_id, lang, prefix)
                        .await,
                    target,
                )
            } else {
                DispatchResponse::current(
                    session_commands::handle_agent(db, args, channel_id, sender_id, lang, prefix)
                        .await,
                    target,
                )
            }
        }
        "new" | "task" | "do" => DispatchResponse::from_command_result(
            session_commands::handle_task(
                db, args, channel_id, sender_id, target, manager, conn_mgr, emitter, bridge, lang,
                prefix, data_dir,
            )
            .await,
        ),
        "sessions" => DispatchResponse::current(
            session_commands::handle_sessions(db, channel_id, sender_id, target, lang, prefix)
                .await,
            target,
        ),
        "resume" => DispatchResponse::current(
            session_commands::handle_resume(
                db, args, channel_id, sender_id, target, manager, conn_mgr, emitter, bridge, lang,
                prefix, data_dir,
            )
            .await,
            target,
        ),
        "cancel" => DispatchResponse::current(
            session_commands::handle_cancel(
                db, channel_id, sender_id, target, conn_mgr, bridge, lang,
            )
            .await,
            target,
        ),
        "approve" => {
            let always = args.eq_ignore_ascii_case("always");
            DispatchResponse::current(
                session_commands::handle_permission_response(
                    true, always, db, channel_id, sender_id, target, conn_mgr, bridge, lang,
                )
                .await,
                target,
            )
        }
        "deny" => DispatchResponse::current(
            session_commands::handle_permission_response(
                false, false, db, channel_id, sender_id, target, conn_mgr, bridge, lang,
            )
            .await,
            target,
        ),

        _ => DispatchResponse::current(
            RichMessage::info(i18n::unknown_command(lang, prefix, &command))
                .with_title(i18n::unknown_command_title(lang)),
            target,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
async fn dispatch_natural_message(
    text: &str,
    prefix: &str,
    db: &DatabaseConnection,
    manager: &ChatChannelManager,
    conn_mgr: &ConnectionManager,
    emitter: &EventEmitter,
    bridge: &Arc<Mutex<SessionBridge>>,
    data_dir: &Path,
    channel_id: i32,
    sender_id: &str,
    sender_name: Option<&str>,
    target: &ChannelMessageTarget,
    lang: Lang,
) -> DispatchResponse {
    let decision =
        natural_router::route_natural_message(db, bridge, channel_id, sender_id, text, lang).await;
    tracing::info!(
        "[ChatChannel] natural route channel={} sender={} decision={:?}",
        channel_id,
        sender_id,
        decision
    );

    match decision {
        NaturalRouteDecision::ContinueSession => DispatchResponse::current(
            session_commands::handle_followup(session_commands::FollowupRequest {
                db,
                text,
                channel_id,
                sender_id,
                target,
                conn_mgr,
                emitter,
                bridge,
                data_dir,
                lang,
                prefix,
            })
            .await,
            target,
        ),
        NaturalRouteDecision::ApprovePermission { always } => DispatchResponse::current(
            session_commands::handle_permission_response(
                true, always, db, channel_id, sender_id, target, conn_mgr, bridge, lang,
            )
            .await,
            target,
        ),
        NaturalRouteDecision::DenyPermission => DispatchResponse::current(
            session_commands::handle_permission_response(
                false, false, db, channel_id, sender_id, target, conn_mgr, bridge, lang,
            )
            .await,
            target,
        ),
        NaturalRouteDecision::CancelSession => DispatchResponse::current(
            session_commands::handle_cancel(
                db, channel_id, sender_id, target, conn_mgr, bridge, lang,
            )
            .await,
            target,
        ),
        NaturalRouteDecision::StartTask {
            task,
            folder_id,
            agent_type,
        } => {
            let _ =
                sender_context_service::update_folder(db, channel_id, sender_id, Some(folder_id))
                    .await;
            let _ = sender_context_service::update_agent(
                db,
                channel_id,
                sender_id,
                Some(natural_router::agent_type_to_wire(agent_type)),
            )
            .await;
            // Build prompt: memory context (recent days) + sender name + task.
            // Memory is only fetched for dedicated-folder channels; returns
            // None quickly for regular channels so there's no extra latency.
            let memory =
                natural_router::build_channel_memory_context(db, channel_id, lang).await;
            let prompt = build_task_prompt(memory.as_deref(), sender_name, &task);
            DispatchResponse::from_command_result(
                session_commands::handle_task(
                    db, &prompt, channel_id, sender_id, target, manager, conn_mgr, emitter,
                    bridge, lang, prefix, data_dir,
                )
                .await,
            )
        }
        NaturalRouteDecision::ShowStatus => {
            DispatchResponse::current(command_handlers::handle_status(manager, lang).await, target)
        }
        NaturalRouteDecision::ShowToday => {
            DispatchResponse::current(command_handlers::handle_today(db, lang).await, target)
        }
        NaturalRouteDecision::SearchHistory { keyword } => DispatchResponse::current(
            command_handlers::handle_search(db, &keyword, lang).await,
            target,
        ),
        NaturalRouteDecision::AskClarification { message } => {
            tracing::info!(
                "[ChatChannel] sending clarification to channel={} sender={}: {}",
                channel_id,
                sender_id,
                message
            );
            DispatchResponse::current(RichMessage::info(message), target)
        }
    }
}

/// Assemble the initial agent prompt from optional pieces:
/// memory context (recent session titles) + sender name + task text.
fn build_task_prompt(
    memory: Option<&str>,
    sender_name: Option<&str>,
    task: &str,
) -> String {
    let sender_prefix = match sender_name {
        Some(name) if !name.is_empty() => format!("[来自 {name}] "),
        _ => String::new(),
    };
    match memory {
        Some(ctx) if !ctx.is_empty() => {
            format!("{ctx}\n{sender_prefix}{task}")
        }
        _ => format!("{sender_prefix}{task}"),
    }
}
