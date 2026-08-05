//! WeCom (企业微信) channel backend, bridged through the `wecom-cli` companion
//! from the wecom-unified suite (npm `@wecom/cli`).
//!
//! The CLI owns credentials (one-time QR-scan auth via `wecom-cli init`) and
//! the message transport; this backend only orchestrates it:
//!
//! - **Receive**: wecom-cli has no push channel, so a poll loop walks
//!   `msg get_msg_chat_list` → `msg get_message` over a sliding time window
//!   and forwards fresh inbound text messages to the command dispatcher.
//! - **Send**: `msg send_message` (text only — rich messages degrade to
//!   plain text). Replies address the originating chat via the message
//!   target; app-initiated notifications go to the configured default chat.
//!
//! Echo suppression: the poll API returns *all* messages including our own.
//! In a direct chat (`chat_type=1`, chatid == peer userid) anything not sent
//! by the peer is ours. Group messages are filtered against the self userids
//! learned from direct chats plus a short-lived record of texts we sent.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, Duration as ChronoDuration, Local, NaiveDateTime, TimeZone};
use tokio::sync::{mpsc, watch, Mutex};

use crate::chat_channel::error::ChatChannelError;
use crate::chat_channel::traits::ChatChannelBackend;
use crate::chat_channel::types::*;

pub const WECOM_CHAT_THREAD_KIND: &str = "wecom_chat";
pub const WECOM_CLI_PACKAGE: &str = "@wecom/cli@0.1.9";

const CLI_TIMEOUT: Duration = Duration::from_secs(60);
const DEFAULT_POLL_INTERVAL_SECS: u64 = 5;
/// Re-read this much history behind the last successful poll so a message
/// that landed while a poll was in flight is never skipped (dedup drops the
/// overlap duplicates).
const POLL_OVERLAP_SECS: i64 = 120;
/// Sent-text echo records expire after this long.
const SENT_ECHO_TTL: Duration = Duration::from_secs(300);
const MAX_SEEN_KEYS: usize = 4096;
const WECOM_TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";
const WECOM_INIT_ARGS: &[&str] = &["init", "--noninteractive", "--no-open"];

pub struct WecomBackend {
    channel_id: i32,
    config: WecomConfig,
    state: Arc<State>,
}

struct State {
    status: Mutex<ChannelConnectionStatus>,
    stop_tx: Mutex<Option<watch::Sender<bool>>>,
    /// Message keys already forwarded (bounded FIFO of hashes).
    seen: Mutex<SeenKeys>,
    /// Userids observed sending in direct chats that are not the peer — us.
    self_userids: Mutex<HashSet<String>>,
    /// Texts we sent recently, for group-chat echo suppression.
    recently_sent: Mutex<VecDeque<(String, Instant)>>,
    /// chat_id → chat_type resolved by probing (1 direct, 2 group).
    chat_types: Mutex<HashMap<String, u8>>,
}

struct SeenKeys {
    order: VecDeque<u64>,
    set: HashSet<u64>,
}

impl SeenKeys {
    fn new() -> Self {
        Self {
            order: VecDeque::new(),
            set: HashSet::new(),
        }
    }

    /// Returns true when the key was newly inserted (i.e. not seen before).
    fn insert(&mut self, key: u64) -> bool {
        if !self.set.insert(key) {
            return false;
        }
        self.order.push_back(key);
        while self.order.len() > MAX_SEEN_KEYS {
            if let Some(old) = self.order.pop_front() {
                self.set.remove(&old);
            }
        }
        true
    }
}

impl WecomBackend {
    pub fn new(channel_id: i32, config: WecomConfig) -> Self {
        Self {
            channel_id,
            config,
            state: Arc::new(State {
                status: Mutex::new(ChannelConnectionStatus::Disconnected),
                stop_tx: Mutex::new(None),
                seen: Mutex::new(SeenKeys::new()),
                self_userids: Mutex::new(HashSet::new()),
                recently_sent: Mutex::new(VecDeque::new()),
                chat_types: Mutex::new(HashMap::new()),
            }),
        }
    }

    fn poll_interval(&self) -> Duration {
        Duration::from_secs(
            self.config
                .poll_interval_secs
                .unwrap_or(DEFAULT_POLL_INTERVAL_SECS)
                .clamp(2, 300),
        )
    }

    async fn send_text_to_chat(
        &self,
        chat_type: u8,
        chatid: &str,
        text: &str,
    ) -> Result<SentMessageId, ChatChannelError> {
        if chatid.trim().is_empty() {
            return Err(ChatChannelError::ConfigurationInvalid(
                "WeCom chat id is empty — configure a default chat or reply within a chat".into(),
            ));
        }
        // wecom-cli caps text.content at 2048 bytes; split long replies.
        for chunk in split_utf8_chunks(text, 2000) {
            let payload = serde_json::json!({
                "chat_type": chat_type,
                "chatid": chatid,
                "msgtype": "text",
                "text": {"content": chunk},
            });
            run_cli_json(&["msg", "send_message", &payload.to_string()]).await?;
            let mut sent = self.state.recently_sent.lock().await;
            sent.push_back((chunk.to_string(), Instant::now()));
            while sent.len() > 64 {
                sent.pop_front();
            }
        }
        Ok(SentMessageId(format!("wecom-{}", uuid::Uuid::new_v4())))
    }

    fn target_chat(&self, target: Option<&ChannelMessageTarget>) -> (u8, String) {
        if let Some(target) = target {
            if let Some(chat_id) = target.chat_id.as_deref().filter(|id| !id.is_empty()) {
                let chat_type = target
                    .provider_payload
                    .as_ref()
                    .and_then(|payload| payload.get("chat_type"))
                    .and_then(|value| value.as_u64())
                    .map(|value| value as u8)
                    .unwrap_or(1);
                return (chat_type, chat_id.to_string());
            }
        }
        (
            self.config.default_chat_type,
            self.config.default_chatid.clone(),
        )
    }
}

#[async_trait]
impl ChatChannelBackend for WecomBackend {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Wecom
    }

    async fn start(
        &self,
        command_tx: mpsc::Sender<IncomingCommand>,
    ) -> Result<(), ChatChannelError> {
        ensure_cli_installed().await?;
        ensure_authorized().await?;

        let (stop_tx, stop_rx) = watch::channel(false);
        {
            let mut guard = self.state.stop_tx.lock().await;
            if let Some(previous) = guard.take() {
                let _ = previous.send(true);
            }
            *guard = Some(stop_tx);
        }

        *self.state.status.lock().await = ChannelConnectionStatus::Connected;
        tokio::spawn(poll_loop(
            self.channel_id,
            self.poll_interval(),
            Arc::clone(&self.state),
            command_tx,
            stop_rx,
        ));
        tracing::info!(
            "[WeCom] channel {} connected (poll interval {:?})",
            self.channel_id,
            self.poll_interval()
        );
        Ok(())
    }

    async fn stop(&self) -> Result<(), ChatChannelError> {
        if let Some(stop_tx) = self.state.stop_tx.lock().await.take() {
            let _ = stop_tx.send(true);
        }
        *self.state.status.lock().await = ChannelConnectionStatus::Disconnected;
        Ok(())
    }

    async fn status(&self) -> ChannelConnectionStatus {
        *self.state.status.lock().await
    }

    async fn send_message(&self, text: &str) -> Result<SentMessageId, ChatChannelError> {
        let (chat_type, chatid) = self.target_chat(None);
        self.send_text_to_chat(chat_type, &chatid, text).await
    }

    async fn send_rich_message(
        &self,
        message: &RichMessage,
    ) -> Result<SentMessageId, ChatChannelError> {
        self.send_message(&message.to_plain_text()).await
    }

    async fn send_rich_message_to(
        &self,
        message: &RichMessage,
        target: &ChannelMessageTarget,
    ) -> Result<SentMessageId, ChatChannelError> {
        let (chat_type, chatid) = self.target_chat(Some(target));
        self.send_text_to_chat(chat_type, &chatid, &message.to_plain_text())
            .await
    }

    async fn test_connection(&self) -> Result<(), ChatChannelError> {
        ensure_cli_installed().await?;
        ensure_authorized().await
    }
}

// ── Poll loop ──

async fn poll_loop(
    channel_id: i32,
    interval: Duration,
    state: Arc<State>,
    command_tx: mpsc::Sender<IncomingCommand>,
    mut stop_rx: watch::Receiver<bool>,
) {
    // Only messages after connect are forwarded — replaying history into the
    // dispatcher would fire tasks for week-old texts.
    let mut window_start = Local::now();
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = ticker.tick() => {}
            _ = stop_rx.changed() => {
                if *stop_rx.borrow() {
                    tracing::info!("[WeCom] channel {channel_id} poll loop stopped");
                    return;
                }
            }
        }

        let now = Local::now();
        let begin = window_start - ChronoDuration::seconds(POLL_OVERLAP_SECS);
        match poll_once(channel_id, &state, &command_tx, begin, now).await {
            Ok(()) => {
                window_start = now;
                if *state.status.lock().await != ChannelConnectionStatus::Connected {
                    *state.status.lock().await = ChannelConnectionStatus::Connected;
                }
            }
            Err(error) => {
                tracing::warn!("[WeCom] channel {channel_id} poll failed: {error}");
                *state.status.lock().await = ChannelConnectionStatus::Error;
            }
        }
    }
}

async fn poll_once(
    channel_id: i32,
    state: &Arc<State>,
    command_tx: &mpsc::Sender<IncomingCommand>,
    begin: DateTime<Local>,
    end: DateTime<Local>,
) -> Result<(), ChatChannelError> {
    let begin_str = begin.format(WECOM_TIME_FORMAT).to_string();
    let end_str = end.format(WECOM_TIME_FORMAT).to_string();

    let mut chats = Vec::new();
    let mut cursor: Option<String> = None;
    for _page in 0..5 {
        let mut payload = serde_json::json!({
            "begin_time": begin_str,
            "end_time": end_str,
        });
        if let Some(cursor) = cursor.as_deref() {
            payload["cursor"] = serde_json::Value::String(cursor.to_string());
        }
        let response = run_cli_json(&["msg", "get_msg_chat_list", &payload.to_string()]).await?;
        if let Some(list) = response.get("chats").and_then(|value| value.as_array()) {
            for chat in list {
                if let Some(chat_id) = chat.get("chat_id").and_then(|value| value.as_str()) {
                    chats.push((
                        chat_id.to_string(),
                        chat.get("chat_name")
                            .and_then(|value| value.as_str())
                            .unwrap_or_default()
                            .to_string(),
                    ));
                }
            }
        }
        cursor = response
            .get("next_cursor")
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .map(String::from);
        let has_more = response
            .get("has_more")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        if cursor.is_none() || !has_more {
            break;
        }
    }

    for (chat_id, chat_name) in chats {
        if let Err(error) = poll_chat(
            channel_id, state, command_tx, &chat_id, &chat_name, &begin_str, &end_str,
        )
        .await
        {
            tracing::warn!("[WeCom] failed to poll chat {chat_id}: {error}");
        }
    }
    Ok(())
}

/// Send the bounded-queue busy notice back to the originating chat so an
/// inbound message is never silently dropped.
async fn send_busy_to_chat(chat_type: u8, chatid: &str) {
    if chatid.trim().is_empty() {
        return;
    }
    let payload = serde_json::json!({
        "chat_type": chat_type,
        "chatid": chatid,
        "msgtype": "text",
        "text": {"content": super::DISPATCHER_BUSY_TEXT},
    });
    if let Err(error) = run_cli_json(&["msg", "send_message", &payload.to_string()]).await {
        tracing::warn!("[WeCom] busy reply to {chatid} failed: {error}");
    }
}
#[allow(clippy::too_many_arguments)]
async fn poll_chat(
    channel_id: i32,
    state: &Arc<State>,
    command_tx: &mpsc::Sender<IncomingCommand>,
    chat_id: &str,
    chat_name: &str,
    begin: &str,
    end: &str,
) -> Result<(), ChatChannelError> {
    let (chat_type, messages) = fetch_chat_messages(state, chat_id, begin, end).await?;

    for message in messages {
        let Some(sender) = message.get("userid").and_then(|value| value.as_str()) else {
            continue;
        };
        let send_time = message
            .get("send_time")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        if message.get("msgtype").and_then(|value| value.as_str()) != Some("text") {
            continue;
        }
        let Some(content) = message
            .pointer("/text/content")
            .and_then(|value| value.as_str())
        else {
            continue;
        };

        // Learn own identity from direct chats: chatid == peer userid there,
        // so any other sender is the authorized account itself.
        if chat_type == 1 && sender != chat_id {
            state.self_userids.lock().await.insert(sender.to_string());
            continue;
        }
        if state.self_userids.lock().await.contains(sender) {
            continue;
        }
        if chat_type == 2 && was_recently_sent(state, content).await {
            continue;
        }

        let key = message_key(chat_id, sender, send_time, content);
        if !state.seen.lock().await.insert(key) {
            continue;
        }

        let target = ChannelMessageTarget {
            channel_id,
            chat_id: Some(chat_id.to_string()),
            thread_key: None,
            thread_kind: Some(WECOM_CHAT_THREAD_KIND.to_string()),
            provider_payload: Some(serde_json::json!({"chat_type": chat_type})),
        };
        let sender_name = fetch_user_display_name(sender).await;
        // Provider message id: prefer the platform's own id when present,
        // else fall back to the deterministic composite key.
        let provider_message_id = message
            .get("msgid")
            .or_else(|| message.get("message_id"))
            .or_else(|| message.get("id"))
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty())
            .map(|v| v.to_string())
            .unwrap_or_else(|| format!("w{}", message_key(chat_id, sender, send_time, content)));
        let command = IncomingCommand {
            channel_id,
            sender_id: sender.to_string(),
            sender_name: sender_name.clone(),
            command_text: content.to_string(),
            callback_data: None,
            target,
            metadata: serde_json::json!({
                "chat_id": chat_id,
                "chat_name": chat_name,
                "chat_type": chat_type,
                "send_time": send_time,
                "sender_name": sender_name,
            }),
            message_trace_id: super::super::dedupe::new_message_trace_id(channel_id),
            provider_message_id: Some(provider_message_id),
            received_at: chrono::Utc::now(),
        };
        // Bounded queue: never block the poll loop; on a full dispatcher queue
        // tell the sender to retry instead of silently dropping the message.
        if let Err(error) = command_tx.try_send(command) {
            match error {
                mpsc::error::TrySendError::Full(_) => {
                    tracing::warn!(
                        "[WeCom] dispatcher queue full for chat {chat_id}; replying busy"
                    );
                    send_busy_to_chat(chat_type, chat_id).await;
                }
                mpsc::error::TrySendError::Closed(_) => {
                    tracing::error!("[WeCom] command channel closed; dropping inbound");
                }
            }
        }
    }
    Ok(())
}

/// Pull a chat's messages, resolving its `chat_type` by probing: the chat
/// list API doesn't expose the type, so try direct (1) first and fall back
/// to group (2) on an API error. The resolved type is cached.
async fn fetch_chat_messages(
    state: &Arc<State>,
    chat_id: &str,
    begin: &str,
    end: &str,
) -> Result<(u8, Vec<serde_json::Value>), ChatChannelError> {
    let cached = state.chat_types.lock().await.get(chat_id).copied();
    let candidates: &[u8] = match cached {
        Some(1) => &[1],
        Some(_) => &[2],
        None => &[1, 2],
    };

    let mut last_error = ChatChannelError::ConnectionFailed("no chat type candidate".into());
    for &chat_type in candidates {
        match fetch_messages_once(chat_id, chat_type, begin, end).await {
            Ok(messages) => {
                state
                    .chat_types
                    .lock()
                    .await
                    .insert(chat_id.to_string(), chat_type);
                return Ok((chat_type, messages));
            }
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
}

async fn fetch_messages_once(
    chat_id: &str,
    chat_type: u8,
    begin: &str,
    end: &str,
) -> Result<Vec<serde_json::Value>, ChatChannelError> {
    let mut messages = Vec::new();
    let mut cursor: Option<String> = None;
    for _page in 0..10 {
        let mut payload = serde_json::json!({
            "chat_type": chat_type,
            "chatid": chat_id,
            "begin_time": begin,
            "end_time": end,
        });
        if let Some(cursor) = cursor.as_deref() {
            payload["cursor"] = serde_json::Value::String(cursor.to_string());
        }
        let response = run_cli_json(&["msg", "get_message", &payload.to_string()]).await?;
        if let Some(list) = response.get("messages").and_then(|value| value.as_array()) {
            messages.extend(list.iter().cloned());
        }
        cursor = response
            .get("next_cursor")
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .map(String::from);
        if cursor.is_none() {
            break;
        }
    }
    Ok(messages)
}

async fn was_recently_sent(state: &Arc<State>, content: &str) -> bool {
    let mut sent = state.recently_sent.lock().await;
    while let Some((_, at)) = sent.front() {
        if at.elapsed() > SENT_ECHO_TTL {
            sent.pop_front();
        } else {
            break;
        }
    }
    sent.iter().any(|(text, _)| text == content)
}

fn message_key(chat_id: &str, sender: &str, send_time: &str, content: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (chat_id, sender, send_time, content).hash(&mut hasher);
    hasher.finish()
}

fn split_utf8_chunks(text: &str, max_bytes: usize) -> Vec<&str> {
    let mut chunks = Vec::new();
    let mut rest = text;
    while rest.len() > max_bytes {
        let mut cut = max_bytes;
        while !rest.is_char_boundary(cut) {
            cut -= 1;
        }
        let (head, tail) = rest.split_at(cut);
        chunks.push(head);
        rest = tail;
    }
    chunks.push(rest);
    chunks
}

// ── User info ──

/// Best-effort: call `wecom-cli contact get_member_info` to resolve a userid
/// into a human-readable display name. Returns `None` on any error so the
/// caller can fall back to the raw userid without surfacing failures.
async fn fetch_user_display_name(userid: &str) -> Option<String> {
    let payload = serde_json::json!({"userid": userid});
    let result = run_cli_json(&["contact", "get_member_info", &payload.to_string()]).await;
    match result {
        Ok(json) => json
            .get("name")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from),
        Err(e) => {
            tracing::debug!("[WeCom] user info fetch skipped for {userid}: {e}");
            None
        }
    }
}

// ── wecom-cli process helpers (also used by the command layer) ──

async fn run_cli_raw(args: &[&str]) -> Result<String, ChatChannelError> {
    let mut command = crate::process::tokio_command("wecom-cli");
    command.args(args);
    command.stdin(std::process::Stdio::null());
    let output = tokio::time::timeout(CLI_TIMEOUT, command.output())
        .await
        .map_err(|_| ChatChannelError::ConnectionFailed("wecom-cli timed out".into()))?
        .map_err(|error| {
            ChatChannelError::ConnectionFailed(format!("failed to run wecom-cli: {error}"))
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ChatChannelError::ConnectionFailed(format!(
            "wecom-cli exited with {}: {}",
            output.status,
            if stderr.trim().is_empty() {
                stdout.trim()
            } else {
                stderr.trim()
            }
        )));
    }
    Ok(stdout)
}

async fn run_cli_json(args: &[&str]) -> Result<serde_json::Value, ChatChannelError> {
    let stdout = run_cli_raw(args).await?;
    let json_start = stdout.find('{').ok_or_else(|| {
        ChatChannelError::ConnectionFailed(format!("wecom-cli returned no JSON: {}", stdout.trim()))
    })?;
    let value: serde_json::Value =
        serde_json::from_str(stdout[json_start..].trim()).map_err(|error| {
            ChatChannelError::ConnectionFailed(format!("wecom-cli JSON parse failed: {error}"))
        })?;
    let errcode = value.get("errcode").and_then(|value| value.as_i64());
    if let Some(code) = errcode.filter(|code| *code != 0) {
        let errmsg = value
            .get("errmsg")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown error");
        return Err(ChatChannelError::ConnectionFailed(format!(
            "wecom-cli errcode {code}: {errmsg}"
        )));
    }
    Ok(value)
}

pub fn cli_installed() -> bool {
    which::which("wecom-cli").is_ok()
}

/// Install `@wecom/cli` through the managed npm, mirror registry first —
/// consistent with the app-wide mainland-China acceleration policy.
pub async fn install_cli() -> Result<(), ChatChannelError> {
    for registry in ["https://registry.npmmirror.com", ""] {
        let mut command = crate::process::tokio_command("npm");
        command.args(["install", "-g", WECOM_CLI_PACKAGE]);
        if !registry.is_empty() {
            command.arg(format!("--registry={registry}"));
        }
        command.stdin(std::process::Stdio::null());
        let result = tokio::time::timeout(Duration::from_secs(300), command.output()).await;
        match result {
            Ok(Ok(output)) if output.status.success() && cli_installed() => return Ok(()),
            Ok(Ok(output)) => {
                tracing::warn!(
                    "[WeCom] npm install via {} failed: {}",
                    if registry.is_empty() {
                        "default registry"
                    } else {
                        registry
                    },
                    String::from_utf8_lossy(&output.stderr).trim()
                );
            }
            Ok(Err(error)) => {
                return Err(ChatChannelError::ConnectionFailed(format!(
                    "failed to run npm (is the Node.js runtime installed?): {error}"
                )));
            }
            Err(_) => {
                tracing::warn!("[WeCom] npm install timed out via {registry}");
            }
        }
    }
    Err(ChatChannelError::ConnectionFailed(
        "failed to install @wecom/cli from both mirror and official registries".into(),
    ))
}

async fn ensure_cli_installed() -> Result<(), ChatChannelError> {
    if cli_installed() {
        return Ok(());
    }
    tracing::info!("[WeCom] wecom-cli not found, installing {WECOM_CLI_PACKAGE}...");
    install_cli().await
}

/// `authorized` / `unauthorized` per wecom-cli. Note "unauthorized" contains
/// "authorized" as a substring — check the negative first.
pub async fn auth_status() -> Result<bool, ChatChannelError> {
    let output = run_cli_raw(&["auth", "show", "--auth-status"]).await?;
    let normalized = output.to_lowercase();
    if normalized.contains("unauthorized") {
        return Ok(false);
    }
    Ok(normalized.contains("authorized"))
}

async fn ensure_authorized() -> Result<(), ChatChannelError> {
    if auth_status().await? {
        return Ok(());
    }
    Err(ChatChannelError::ConfigurationInvalid(
        "wecom-cli is not authorized yet — start the QR-code authorization from channel settings"
            .into(),
    ))
}

/// Liveness of the detached `wecom-cli init` process started by [`start_auth`].
///
/// The QR scan is completed by that process, not by us: it holds the pending
/// authorization and writes the credential file once the scan lands. If it
/// dies early the UI would otherwise poll [`auth_status`] forever, so its
/// state is published here.
#[derive(Debug, Clone, Default)]
pub struct AuthProcessState {
    pub running: bool,
    /// Last line(s) the process emitted before exiting non-zero, if any.
    pub last_error: Option<String>,
}

static AUTH_PROCESS: std::sync::LazyLock<std::sync::Mutex<AuthProcessState>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(AuthProcessState::default()));

pub fn auth_process_state() -> AuthProcessState {
    AUTH_PROCESS
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_default()
}

fn set_auth_process(running: bool, last_error: Option<String>) {
    if let Ok(mut guard) = AUTH_PROCESS.lock() {
        *guard = AuthProcessState {
            running,
            last_error,
        };
    }
}

/// Kick off `wecom-cli init --noninteractive` in the background and return
/// the authorization link parsed from its output. The process keeps running
/// until the user scans; poll [`auth_status`] to observe completion.
///
/// Both pipes must be drained for the child's entire lifetime, not just until
/// the link appears. `wecom-cli init` keeps writing after the link (the ASCII
/// QR block, then `等待扫码中...`) — closing the read end makes those writes
/// fail and the process aborts (exit 101) within a fraction of a second, long
/// before the user can scan. An undrained stderr would stall it the same way
/// once the pipe buffer fills.
pub async fn start_auth() -> Result<String, ChatChannelError> {
    ensure_cli_installed().await?;
    if auth_status().await.unwrap_or(false) {
        return Err(ChatChannelError::ConfigurationInvalid(
            "wecom-cli is already authorized".into(),
        ));
    }

    let mut command = crate::process::tokio_command("wecom-cli");
    command.args(wecom_init_args());
    command.stdin(std::process::Stdio::null());
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());
    let mut child = command.spawn().map_err(|error| {
        ChatChannelError::ConnectionFailed(format!("failed to run wecom-cli init: {error}"))
    })?;

    let stdout = child.stdout.take().ok_or_else(|| {
        ChatChannelError::ConnectionFailed("wecom-cli init produced no stdout".into())
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        ChatChannelError::ConnectionFailed("wecom-cli init produced no stderr".into())
    })?;

    set_auth_process(true, None);

    // The QR is just the link rendered as ASCII, so the first https:// line is
    // what the UI needs — but the reader keeps running past it so the child can
    // continue writing while it waits for the scan.
    let (link_tx, link_rx) = tokio::sync::oneshot::channel::<String>();
    tokio::spawn(async move {
        use tokio::io::AsyncBufReadExt;
        let mut lines = tokio::io::BufReader::new(stdout).lines();
        let mut link_tx = Some(link_tx);
        let mut tail = String::new();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(sender) = link_tx.take() {
                match extract_https_link(&line) {
                    Some(link) => {
                        let _ = sender.send(link);
                    }
                    None => link_tx = Some(sender),
                }
            }
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                tracing::debug!("[WeCom] init: {trimmed}");
                tail = trimmed.to_string();
            }
        }
        tracing::info!("[WeCom] init stdout closed (last line: {tail})");
    });

    // Drained purely to keep the child from blocking on a full pipe; the
    // final lines double as the failure message when it exits non-zero.
    let stderr_tail = Arc::new(Mutex::new(String::new()));
    let stderr_tail_writer = Arc::clone(&stderr_tail);
    tokio::spawn(async move {
        use tokio::io::AsyncBufReadExt;
        let mut lines = tokio::io::BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            tracing::debug!("[WeCom] init stderr: {trimmed}");
            let mut tail = stderr_tail_writer.lock().await;
            *tail = trimmed.to_string();
        }
    });

    tokio::spawn(async move {
        let status = child.wait().await;
        let detail = stderr_tail.lock().await.clone();
        match status {
            Ok(status) if status.success() => {
                tracing::info!("[WeCom] init finished (authorization complete)");
                set_auth_process(false, None);
            }
            Ok(status) => {
                tracing::warn!("[WeCom] init exited with {status}: {detail}");
                set_auth_process(
                    false,
                    Some(if detail.is_empty() {
                        format!("wecom-cli init exited with {status}")
                    } else {
                        format!("wecom-cli init exited with {status}: {detail}")
                    }),
                );
            }
            Err(error) => {
                tracing::warn!("[WeCom] init wait failed: {error}");
                set_auth_process(false, Some(format!("wecom-cli init failed: {error}")));
            }
        }
    });

    match tokio::time::timeout(Duration::from_secs(30), link_rx).await {
        Ok(Ok(link)) => Ok(link),
        // Sender dropped: the reader hit EOF without a link, i.e. the process
        // exited early.
        Ok(Err(_)) => Err(ChatChannelError::ConnectionFailed(
            "wecom-cli init ended without printing an authorization link".into(),
        )),
        Err(_) => Err(ChatChannelError::ConnectionFailed(
            "timed out waiting for the wecom-cli authorization link".into(),
        )),
    }
}

fn extract_https_link(line: &str) -> Option<String> {
    let start = line.find("https://")?;
    let rest = &line[start..];
    let end = rest
        .find(|ch: char| ch.is_whitespace() || ch == '"' || ch == '\'')
        .unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

fn wecom_init_args() -> &'static [&'static str] {
    WECOM_INIT_ARGS
}

/// Parse a wecom timestamp; used to keep the API surface honest in tests.
#[allow(dead_code)]
fn parse_wecom_time(value: &str) -> Option<DateTime<Local>> {
    NaiveDateTime::parse_from_str(value, WECOM_TIME_FORMAT)
        .ok()
        .and_then(|naive| Local.from_local_datetime(&naive).single())
}
