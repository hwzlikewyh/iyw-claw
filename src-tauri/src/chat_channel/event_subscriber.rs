use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use sea_orm::DatabaseConnection;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use super::i18n::Lang;
use super::manager::ChatChannelManager;
use super::message_formatter;
use super::session_bridge::SessionBridge;
use super::types::RichMessage;
use crate::acp::internal_bus::InternalEventBus;
use crate::acp::types::{AcpEvent, EventEnvelope};
use crate::db::service::{
    app_metadata_service, chat_channel_message_log_service, chat_channel_service,
};

/// Minimum interval between pushes for the same event type per channel (debounce).
const DEBOUNCE_SECS: u64 = 5;

/// Events that export user-authored content (the prompt text itself) to
/// external sinks — IM channels, webhooks, and the outbound message log. They
/// are NOT part of the default ("all events") feed: a null/absent filter
/// EXCLUDES them, so an install that never customized its filter does not begin
/// forwarding prompt text after upgrade. The user must enable them
/// deliberately, which persists an explicit filter list containing the id.
const DEFAULT_OFF_EVENTS: &[&str] = &["user_prompt_sent"];
/// How often to refresh cached config from DB.
const CONFIG_CACHE_TTL_SECS: u64 = 30;

const MESSAGE_LANGUAGE_KEY: &str = "chat_message_language";
const EVENT_FILTER_KEY: &str = "chat_event_filter";
const EVENT_WEBHOOKS_KEY: &str = "chat_event_webhooks";

/// Bumped whenever the Events-tab config (event filter, webhooks, message
/// language) is written. The subscriber's config cache compares this against
/// its last-seen value and refreshes immediately on change, instead of waiting
/// out the `CONFIG_CACHE_TTL_SECS` window — so e.g. disabling a webhook stops
/// deliveries on the next event rather than up to 30s later.
static EVENT_CONFIG_EPOCH: AtomicU64 = AtomicU64::new(0);

/// Signal that the Events-tab config changed; call after a successful write.
pub fn bump_event_config_epoch() {
    EVENT_CONFIG_EPOCH.fetch_add(1, Ordering::Relaxed);
}

struct CachedChannel {
    id: i32,
    event_filter_json: Option<String>,
}

struct EventConfigCache {
    lang: Lang,
    global_filter: Option<Vec<String>>,
    /// Whether `global_filter` currently reflects a clean read for the latest
    /// observed config. While it does NOT, the filter is UNKNOWN and
    /// `process_envelope` fails CLOSED (suppresses all pushes) rather than fall
    /// back to the cached value. It is false in two cases:
    ///   - cold start, before any clean read — a startup DB error or a corrupt
    ///     stored value must not broadcast events a restrictive config would have
    ///     blocked, so we must not fall back to the broad default set; and
    ///   - after a config change (epoch bump) whose new filter could not be
    ///     loaded — the cached filter predates the change and may be BROADER than
    ///     the user's new intent, so it must not keep gating delivery.
    ///
    /// `global_filter == None` is ambiguous on its own (unread vs. cleanly-read
    /// default), and the cached value is ambiguous after a change (stale vs.
    /// current); this flag disambiguates both.
    filter_known: bool,
    enabled_channels: Vec<CachedChannel>,
    /// Channel-agnostic webhook sinks. Receive the same globally-filtered event
    /// feed as IM channels, but are not debounced and ignore the per-channel
    /// filter (an automation consumer wants the complete stream).
    webhooks: Vec<String>,
    last_refresh: Instant,
    /// Value of `EVENT_CONFIG_EPOCH` at the last refresh; a mismatch forces an
    /// immediate refresh even within the TTL window.
    last_epoch: u64,
}

impl EventConfigCache {
    fn new() -> Self {
        Self {
            lang: Lang::default(),
            global_filter: None,
            // Unknown until the first clean read; process_envelope fails closed.
            filter_known: false,
            enabled_channels: Vec::new(),
            webhooks: Vec::new(),
            // Force refresh on first use
            last_refresh: Instant::now() - Duration::from_secs(CONFIG_CACHE_TTL_SECS + 1),
            last_epoch: 0,
        }
    }

    async fn refresh_if_needed(&mut self, db: &DatabaseConnection) {
        self.refresh_with_epoch(db, EVENT_CONFIG_EPOCH.load(Ordering::Relaxed))
            .await;
    }

    /// Refresh against an explicitly-supplied config epoch. `refresh_if_needed`
    /// passes the live `EVENT_CONFIG_EPOCH`; tests pass a fixed value so the
    /// `config_changed` decision is deterministic and doesn't depend on the
    /// process-global atomic (which other parallel tests mutate).
    async fn refresh_with_epoch(&mut self, db: &DatabaseConnection, epoch: u64) {
        // Skip only when neither the TTL has elapsed NOR the config epoch moved.
        // A config write (filter, webhooks, or language) bumps the epoch. When it
        // no longer matches the epoch of our last clean filter read, a change is
        // pending and the cached global_filter may already be out of date.
        let config_changed = epoch != self.last_epoch;
        if !config_changed
            && self.last_refresh.elapsed() < Duration::from_secs(CONFIG_CACHE_TTL_SECS)
        {
            return;
        }

        if let Ok(Some(val)) = app_metadata_service::get_value(db, MESSAGE_LANGUAGE_KEY).await {
            self.lang = Lang::from_str_lossy(&val);
        }

        // Global event filter — the gate governing ALL delivery. Treat it as
        // KNOWN (and only then advance the epoch/TTL below) when the read AND
        // parse both succeed. A transient DB error or a corrupt stored value
        // leaves the prior value untouched and the read marked failed, so:
        //   - at cold start the filter stays UNKNOWN and process_envelope fails
        //     CLOSED (suppresses) rather than falling back to the broad default;
        //   - after a config change we keep retrying on the next event instead of
        //     holding a possibly-stale (broader) filter for the whole TTL window,
        //     and (see below) fail CLOSED meanwhile.
        // Successful-read cases:
        //   - no row / JSON "null" → the default set (None)
        //   - JSON [..]            → an explicit allow-list
        let filter_ok = match app_metadata_service::get_value(db, EVENT_FILTER_KEY).await {
            Ok(None) => {
                self.global_filter = None;
                self.filter_known = true;
                true
            }
            Ok(Some(json)) => match serde_json::from_str::<Option<Vec<String>>>(&json) {
                Ok(parsed) => {
                    self.global_filter = parsed;
                    self.filter_known = true;
                    true
                }
                // Corrupt value: keep the prior cached value, retry later.
                Err(_) => false,
            },
            // DB error: keep the prior cached value, retry later.
            Err(_) => false,
        };

        // A config change is pending (epoch advanced) but the new filter could
        // not be loaded. The cached filter predates the change and may be BROADER
        // than the user's new intent — keeping it as the delivery gate would leak
        // events the change might have disabled (e.g. a just-toggled-off
        // user_prompt_sent). Mark the filter UNKNOWN so process_envelope fails
        // CLOSED until a clean read for this change lands. A pure TTL refresh that
        // fails (epoch unchanged) instead keeps the still-valid prior filter, so a
        // transient blip doesn't drop legitimate notifications when nothing changed.
        if !filter_ok && config_changed {
            self.filter_known = false;
        }

        // Webhook delivery set — only ENABLED URLs. Absent/unparseable means no
        // webhooks configured.
        self.webhooks = app_metadata_service::get_value(db, EVENT_WEBHOOKS_KEY)
            .await
            .ok()
            .flatten()
            .map(|json| super::webhook::enabled_webhook_urls(&json))
            .unwrap_or_default();

        if let Ok(channels) = chat_channel_service::list_enabled(db).await {
            self.enabled_channels = channels
                .into_iter()
                .map(|ch| CachedChannel {
                    id: ch.id,
                    event_filter_json: ch.event_filter_json,
                })
                .collect();
        }

        // Only mark the cache refreshed for this epoch/TTL window when the global
        // filter — the gate governing ALL delivery — loaded cleanly. A failed
        // filter read leaves the cache eligible to retry on the very next event
        // instead of holding a possibly-stale (or still-unknown) filter until the
        // TTL elapses. The other reads above already keep-prior / fail-closed on
        // their own errors, so re-reading them while retrying is harmless.
        if filter_ok {
            self.last_refresh = Instant::now();
            self.last_epoch = epoch;
        }
    }
}

pub fn spawn_event_subscriber(
    bus: Arc<InternalEventBus>,
    manager: ChatChannelManager,
    db_conn: DatabaseConnection,
    bridge: Arc<Mutex<SessionBridge>>,
) -> JoinHandle<()> {
    // Subscribe synchronously before the spawn so the broadcast buffer
    // catches any events emitted in the gap between `start_background`
    // returning and the spawned task's first `rx.recv().await` poll.
    let mut rx = bus.subscribe();
    let metrics = Arc::clone(bus.metrics());

    tokio::spawn(async move {
        let mut last_push: HashMap<(i32, String), Instant> = HashMap::new();
        let mut config = EventConfigCache::new();
        // One reqwest client, reused (and cheaply cloned) for every webhook POST.
        let webhook_client = super::webhook::make_webhook_client();

        loop {
            let envelope_arc = match rx.recv().await {
                Ok(e) => e,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("[ChatChannel] event subscriber lagged by {n} messages");
                    metrics.lagged_count.fetch_add(n, Ordering::Relaxed);
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::info!("[ChatChannel] internal bus closed, stopping subscriber");
                    break;
                }
            };

            config.refresh_if_needed(&db_conn).await;

            // Prune stale debounce entries
            last_push.retain(|_, t| t.elapsed() < Duration::from_secs(DEBOUNCE_SECS * 2));

            process_envelope(
                envelope_arc.as_ref(),
                &bridge,
                &manager,
                &db_conn,
                &config,
                &mut last_push,
                &webhook_client,
            )
            .await;
        }
    })
}

/// Handle a single bus envelope: map it to a chat-channel push, apply the
/// global + per-channel event filters and the per-(channel, event) debounce,
/// then fan out to the enabled channels and log the outcome.
///
/// Extracted from the subscriber loop so the filter/dedup/debounce logic is
/// unit-testable against a recording backend.
#[allow(clippy::too_many_arguments)]
async fn process_envelope(
    envelope: &EventEnvelope,
    bridge: &Arc<Mutex<SessionBridge>>,
    manager: &ChatChannelManager,
    db_conn: &DatabaseConnection,
    config: &EventConfigCache,
    last_push: &mut HashMap<(i32, String), Instant>,
    webhook_client: &reqwest::Client,
) {
    let Some((event_type, msg)) = parse_acp_event(&envelope.payload, config.lang) else {
        return;
    };

    // Fail closed unless the global filter reflects a clean read for the latest
    // config. An unread/unreadable filter (cold-start DB error or corrupt value)
    // must NOT fall back to the broad default set, and a filter left stale by a
    // config change whose new value couldn't be loaded must NOT keep gating with a
    // possibly-broader rule. Both leave `filter_known == false`; neither
    // `global_filter == None` nor the cached value can distinguish those states on
    // their own, so gate on the explicit known flag.
    if !config.filter_known {
        return;
    }

    // IM pushes require an explicitly saved event filter. A never-configured
    // filter (None) means channels only carry their own sessions (the session
    // relay) — desktop/GUI session events must not leak into IM chats the
    // user never wired up for notifications. Webhooks are their own explicit
    // opt-in, so they keep the historical default set (everything except the
    // prompt-text-exporting DEFAULT_OFF_EVENTS) when no filter was saved.
    let im_allowed = matches!(&config.global_filter, Some(filter) if filter.contains(&event_type));
    let webhook_allowed = !config.webhooks.is_empty()
        && match &config.global_filter {
            Some(filter) => filter.contains(&event_type),
            None => !DEFAULT_OFF_EVENTS.contains(&event_type.as_str()),
        };
    if !im_allowed && !webhook_allowed {
        return;
    }

    // A permission request from a session that was started FROM a chat channel
    // is already handled interactively (with `/approve`, `/deny`) by the session
    // relay, scoped to its owning channel. Suppress the generic global push for
    // those connections so they aren't double-notified — the global event feed
    // exists for the desktop / web sessions the user isn't driving from chat.
    if event_type == "permission_request"
        && bridge.lock().await.get(&envelope.connection_id).is_some()
    {
        return;
    }

    // Webhook fan-out: channel-agnostic, shares the global gates above but is
    // independent of the per-channel filter and the debounce below. Built once
    // and delivered fire-and-forget so an unreachable endpoint can't stall the
    // subscriber loop. Runs even with zero enabled IM channels.
    if webhook_allowed {
        let payload =
            super::webhook::build_webhook_payload(&event_type, &envelope.connection_id, &msg);
        super::webhook::spawn_webhook_delivery(
            webhook_client.clone(),
            config.webhooks.clone(),
            payload,
        );
    }

    if !im_allowed {
        return;
    }

    // Some events bypass the per-(channel, event) debounce. That debounce
    // throttles high-frequency events like turn_complete, but these are discrete,
    // individually-meaningful events that must each deliver:
    //   - permission_request: a blocking gate; a second gate on the same
    //     connection (sequential) or a concurrent agent's gate within the 5s
    //     window would otherwise be dropped — and a blocked agent emits no
    //     further event to re-trigger the lost nudge.
    //   - user_prompt_sent: each user message is a distinct action a consumer
    //     wants to see; coalescing two messages sent within 5s would silently
    //     swallow the second.
    //   - question_request: like permission_request, a blocking interactive
    //     gate (the agent is parked on ask_user_question); a second gate within
    //     the 5s window would be dropped with no later event to re-trigger it.
    let debounced = !matches!(
        event_type.as_str(),
        "permission_request" | "user_prompt_sent" | "question_request"
    );

    for ch in &config.enabled_channels {
        // Per-channel event filter
        if let Some(filter_json) = &ch.event_filter_json {
            if let Ok(filter) = serde_json::from_str::<Vec<String>>(filter_json) {
                if !filter.contains(&event_type) {
                    continue;
                }
            }
        }

        // Debounce: skip if the same event type was pushed to this channel
        // recently (permission_request is exempt — see above).
        let key = (ch.id, event_type.clone());
        let now = Instant::now();
        if debounced {
            if let Some(last) = last_push.get(&key) {
                if now.duration_since(*last) < Duration::from_secs(DEBOUNCE_SECS) {
                    continue;
                }
            }
        }

        // Send
        let send_result = manager.send_to_channel(ch.id, &msg).await;
        let (status, error_detail) = match &send_result {
            Ok(_) => {
                // Only update the debounce timestamp on success, and only for
                // debounced event types.
                if debounced {
                    last_push.insert(key, now);
                }
                ("sent", None)
            }
            Err(e) => ("failed", Some(e.to_string())),
        };

        let _ = chat_channel_message_log_service::create_log(
            db_conn,
            ch.id,
            "outbound",
            "event_push",
            &msg.to_plain_text(),
            status,
            error_detail,
        )
        .await;
    }
}

/// Map an ACP event into the chat-channel push tuple. Pattern-match on the
/// typed `AcpEvent` variant — Phase 5 source-of-truth replaces the prior
/// JSON `type`-string dispatch (which paid `serde_json::from_value` per
/// event for the global broadcaster path).
fn parse_acp_event(payload: &AcpEvent, lang: Lang) -> Option<(String, RichMessage)> {
    match payload {
        AcpEvent::TurnComplete {
            stop_reason,
            agent_type,
            ..
        } => {
            let _ = (stop_reason, agent_type, lang);
            None
        }
        AcpEvent::Error {
            message,
            agent_type,
            ..
        } => Some((
            "error".to_string(),
            message_formatter::format_agent_error(agent_type, message, lang),
        )),
        AcpEvent::PermissionRequest { tool_call, .. } => Some((
            "permission_request".to_string(),
            message_formatter::format_permission_request(tool_call, lang),
        )),
        AcpEvent::UserPromptSent { text_preview } => Some((
            "user_prompt_sent".to_string(),
            message_formatter::format_user_prompt_sent(text_preview, lang),
        )),
        AcpEvent::QuestionRequest { questions, .. } => Some((
            "question_request".to_string(),
            message_formatter::format_question_request(questions, lang),
        )),
        _ => None,
    }
}

