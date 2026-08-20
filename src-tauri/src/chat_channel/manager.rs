use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use sea_orm::DatabaseConnection;
use tokio::sync::{mpsc, Mutex};

use super::error::ChatChannelError;
use super::session_bridge::SessionBridge;
use super::traits::ChatChannelBackend;
use super::types::*;
use crate::acp::manager::ConnectionManager;
use crate::web::event_bridge::{EventEmitter, WebEventBroadcaster};

struct ActiveChannel {
    id: i32,
    generation: u64,
    name: String,
    channel_type: ChannelType,
    backend: Arc<dyn ChatChannelBackend>,
    /// Timestamp of the last inbound message accepted by the dispatcher.
    last_inbound_at: Option<DateTime<Utc>>,
    /// Number of inbound messages accepted by the dispatcher (drives the
    /// `inbound_verified` readiness stage without executing user tasks).
    inbound_count: u64,
}

/// Inner state shared across clones.
struct Inner {
    channels: Mutex<HashMap<i32, ActiveChannel>>,
    command_tx: mpsc::Sender<IncomingCommand>,
    command_rx: Mutex<Option<mpsc::Receiver<IncomingCommand>>>,
    runtime_tx: mpsc::Sender<ChannelRuntimeEvent>,
    runtime_rx: Mutex<Option<mpsc::Receiver<ChannelRuntimeEvent>>>,
    next_generation: AtomicU64,
    broadcaster: Mutex<Option<Arc<WebEventBroadcaster>>>,
    data_dir: Mutex<Option<PathBuf>>,
}

pub struct ChatChannelManager {
    inner: Arc<Inner>,
}

impl Default for ChatChannelManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ChatChannelManager {
    pub fn new() -> Self {
        let (command_tx, command_rx) = mpsc::channel(256);
        let (runtime_tx, runtime_rx) = mpsc::channel(64);
        Self {
            inner: Arc::new(Inner {
                channels: Mutex::new(HashMap::new()),
                command_tx,
                command_rx: Mutex::new(Some(command_rx)),
                runtime_tx,
                runtime_rx: Mutex::new(Some(runtime_rx)),
                next_generation: AtomicU64::new(0),
                broadcaster: Mutex::new(None),
                data_dir: Mutex::new(None),
            }),
        }
    }

    /// Shallow clone sharing the same state (like ConnectionManager::clone_ref).
    pub fn clone_ref(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }

    pub fn command_sender(&self) -> mpsc::Sender<IncomingCommand> {
        self.inner.command_tx.clone()
    }

    pub async fn set_data_dir(&self, data_dir: PathBuf) {
        *self.inner.data_dir.lock().await = Some(data_dir);
    }

    pub async fn data_dir(&self) -> Option<PathBuf> {
        self.inner.data_dir.lock().await.clone()
    }

    /// Take the command receiver (can only be called once, at startup).
    pub async fn take_command_receiver(&self) -> Option<mpsc::Receiver<IncomingCommand>> {
        self.inner.command_rx.lock().await.take()
    }

    async fn take_runtime_receiver(&self) -> Option<mpsc::Receiver<ChannelRuntimeEvent>> {
        self.inner.runtime_rx.lock().await.take()
    }

    fn next_generation(&self) -> u64 {
        self.inner.next_generation.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Emit a status change event to the frontend via broadcaster.
    async fn emit_status_event(&self, channel_id: i32, status: &str) {
        if let Some(broadcaster) = self.inner.broadcaster.lock().await.as_ref() {
            broadcaster.send(
                "chat-channel://status",
                &serde_json::json!({
                    "channel_id": channel_id,
                    "status": status,
                }),
            );
        }
    }

    /// Public wrapper so the reconcile path can push runtime status changes
    /// (e.g. `error` after a failed reconnect) to the UI.
    pub async fn emit_channel_status(&self, channel_id: i32, status: &str) {
        self.emit_status_event(channel_id, status).await;
    }

    pub async fn add_channel(
        &self,
        id: i32,
        name: String,
        channel_type: ChannelType,
        backend: Box<dyn ChatChannelBackend>,
    ) -> Result<(), ChatChannelError> {
        self.upsert_channel(id, name, channel_type, Arc::from(backend))
            .await
    }

    /// Stop any existing backend for the channel (prevents task leak on
    /// duplicate connect), start the new one, then publish the entry.
    pub async fn upsert_channel(
        &self,
        id: i32,
        name: String,
        channel_type: ChannelType,
        backend: Arc<dyn ChatChannelBackend>,
    ) -> Result<(), ChatChannelError> {
        let generation = self.next_generation();
        let old = self.inner.channels.lock().await.remove(&id);
        if let Some(existing) = old {
            let _ = existing.backend.stop().await;
        }

        self.start_and_publish(id, generation, name, channel_type, backend)
            .await
    }

    /// Detach the running backend for a channel so the caller can restore it
    /// (last-known-good) if a reconfiguration fails. The backend is stopped.
    pub async fn take_backend(&self, id: i32) -> Option<Arc<dyn ChatChannelBackend>> {
        let removed = self.inner.channels.lock().await.remove(&id);
        if let Some(channel) = removed {
            let _ = channel.backend.stop().await;
            Some(channel.backend)
        } else {
            None
        }
    }

    /// Restore a previously detached backend (used by safe-reconnect rollback).
    pub async fn restore_backend(
        &self,
        id: i32,
        name: String,
        channel_type: ChannelType,
        backend: Arc<dyn ChatChannelBackend>,
    ) -> Result<(), ChatChannelError> {
        let generation = self.next_generation();
        let old = self.inner.channels.lock().await.remove(&id);
        if let Some(existing) = old {
            let _ = existing.backend.stop().await;
        }
        self.start_and_publish(id, generation, name, channel_type, backend)
            .await
    }

    /// Publish the channel only after startup completed. `start` may launch a
    /// reconnecting transport, so the status notification must reflect the
    /// backend's observed state rather than treating a successful spawn as a
    /// completed connection.
    async fn start_and_publish(
        &self,
        id: i32,
        generation: u64,
        name: String,
        channel_type: ChannelType,
        backend: Arc<dyn ChatChannelBackend>,
    ) -> Result<(), ChatChannelError> {
        let command_tx = self.inner.command_tx.clone();
        let runtime_tx = self.inner.runtime_tx.clone();
        if let Err(error) = backend.start(command_tx, runtime_tx, generation).await {
            tracing::warn!(
                channel_id = id,
                channel_type = %channel_type,
                generation,
                stage = "startup",
                error_category = error.category(),
                error = %error,
                "[ChatChannel] backend startup failed"
            );
            return Err(error);
        }

        let transport_status = backend.status().await;
        self.inner.channels.lock().await.insert(
            id,
            ActiveChannel {
                id,
                generation,
                name,
                channel_type,
                backend,
                last_inbound_at: None,
                inbound_count: 0,
            },
        );
        let status = connection_status_name(transport_status);
        tracing::info!(
            channel_id = id,
            channel_type = %channel_type,
            generation,
            stage = "startup",
            transport_status = status,
            "[ChatChannel] backend startup completed"
        );
        self.emit_status_event(id, status).await;
        Ok(())
    }

    pub async fn remove_channel(&self, id: i32) -> Result<(), ChatChannelError> {
        let candidate = self
            .inner
            .channels
            .lock()
            .await
            .get(&id)
            .map(|channel| (channel.generation, channel.backend.clone()));
        if let Some((generation, backend)) = candidate {
            backend.stop().await?;
            let removed = {
                let mut channels = self.inner.channels.lock().await;
                if channels
                    .get(&id)
                    .is_some_and(|channel| channel.generation == generation)
                {
                    channels.remove(&id);
                    true
                } else {
                    false
                }
            };
            if !removed {
                return Ok(());
            }
            self.emit_status_event(id, "disconnected").await;
        }
        Ok(())
    }

    pub async fn stop_all(&self) {
        let drained: Vec<ActiveChannel> = {
            let mut channels = self.inner.channels.lock().await;
            channels.drain().map(|(_, ch)| ch).collect()
        };
        for channel in drained {
            let _ = channel.backend.stop().await;
        }
    }

    pub async fn send_to_channel(
        &self,
        channel_id: i32,
        message: &RichMessage,
    ) -> Result<SentMessageId, ChatChannelError> {
        let backend = {
            let channels = self.inner.channels.lock().await;
            channels
                .get(&channel_id)
                .ok_or(ChatChannelError::NotFound(channel_id))?
                .backend
                .clone()
        };
        backend.send_rich_message(message).await
    }

    pub(super) async fn backend_for(
        &self,
        channel_id: i32,
    ) -> Result<Arc<dyn ChatChannelBackend>, ChatChannelError> {
        self.inner
            .channels
            .lock()
            .await
            .get(&channel_id)
            .map(|channel| channel.backend.clone())
            .ok_or(ChatChannelError::NotFound(channel_id))
    }

    pub async fn send_to_all(&self, message: &RichMessage) {
        let backends: Vec<Arc<dyn ChatChannelBackend>> = {
            let channels = self.inner.channels.lock().await;
            channels.values().map(|ch| ch.backend.clone()).collect()
        };
        for backend in backends {
            let _ = backend.send_rich_message(message).await;
        }
    }

    /// Record that the dispatcher accepted an inbound message, feeding the
    /// `inbound_verified` readiness stage.
    pub async fn record_inbound(&self, channel_id: i32, received_at: DateTime<Utc>) {
        let mut channels = self.inner.channels.lock().await;
        if let Some(channel) = channels.get_mut(&channel_id) {
            channel.last_inbound_at = Some(received_at);
            channel.inbound_count = channel.inbound_count.saturating_add(1);
        }
    }

    pub async fn inbound_stats(&self, channel_id: i32) -> (Option<DateTime<Utc>>, u64) {
        let channels = self.inner.channels.lock().await;
        match channels.get(&channel_id) {
            Some(channel) => (channel.last_inbound_at, channel.inbound_count),
            None => (None, 0),
        }
    }

    pub async fn get_status(&self) -> Vec<crate::models::ChannelStatusInfo> {
        let entries: Vec<(i32, String, String, Arc<dyn ChatChannelBackend>)> = {
            let channels = self.inner.channels.lock().await;
            channels
                .values()
                .map(|ch| {
                    (
                        ch.id,
                        ch.name.clone(),
                        ch.channel_type.to_string(),
                        ch.backend.clone(),
                    )
                })
                .collect()
        };
        let mut result = Vec::with_capacity(entries.len());
        for (id, name, ct, backend) in entries {
            let status = backend.status().await;
            result.push(crate::models::ChannelStatusInfo {
                channel_id: id,
                name,
                channel_type: ct,
                status: serde_json::to_value(status)
                    .ok()
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_else(|| "unknown".to_string()),
            });
        }
        result
    }

    pub async fn test_channel(&self, id: i32) -> Result<(), ChatChannelError> {
        let backend = {
            let channels = self.inner.channels.lock().await;
            channels
                .get(&id)
                .ok_or(ChatChannelError::NotFound(id))?
                .backend
                .clone()
        };
        if backend.status().await == ChannelConnectionStatus::Connected {
            return Ok(());
        }
        backend.test_connection().await
    }

    pub async fn is_connected(&self, id: i32) -> bool {
        self.connection_status(id).await == Some(ChannelConnectionStatus::Connected)
    }

    pub async fn connection_status(&self, id: i32) -> Option<ChannelConnectionStatus> {
        let backend = {
            let channels = self.inner.channels.lock().await;
            channels.get(&id).map(|ch| ch.backend.clone())
        };
        match backend {
            Some(backend) => Some(backend.status().await),
            None => None,
        }
    }

    pub async fn connection_status_for_generation(
        &self,
        id: i32,
        generation: u64,
    ) -> Option<ChannelConnectionStatus> {
        let backend = {
            let channels = self.inner.channels.lock().await;
            channels
                .get(&id)
                .filter(|channel| channel.generation == generation)
                .map(|channel| channel.backend.clone())
        };
        match backend {
            Some(backend) => Some(backend.status().await),
            None => None,
        }
    }

    pub(crate) async fn commit_runtime_if_current<T>(
        &self,
        id: i32,
        generation: u64,
        expected: ChannelConnectionStatus,
        operation: impl Future<Output = T>,
    ) -> Option<T> {
        let channels = self.inner.channels.lock().await;
        let backend = channels
            .get(&id)
            .filter(|channel| channel.generation == generation)?
            .backend
            .clone();
        if backend.status().await != expected {
            return None;
        }
        let result = operation.await;
        drop(channels);
        Some(result)
    }

    /// Start background tasks (event subscriber + command dispatcher) and
    /// reconcile all enabled channels from DB.
    ///
    /// `broadcaster` continues to back the `*Status* / *Inbound*` JSON
    /// events the ChatChannel itself emits (still consumed by the WS
    /// firehose). `bus` carries typed `Arc<EventEnvelope>` to the two
    /// ACP-event-driven subscribers (`event_subscriber`,
    /// `session_event_subscriber`). Phase 5 split: ACP-shaped data goes
    /// through the typed bus; chat-channel-shaped data stays on the JSON
    /// broadcaster.
    pub async fn start_background(
        &self,
        broadcaster: Arc<WebEventBroadcaster>,
        bus: Arc<crate::acp::InternalEventBus>,
        db_conn: DatabaseConnection,
        data_dir: PathBuf,
        conn_mgr: ConnectionManager,
        emitter: EventEmitter,
    ) {
        self.set_data_dir(data_dir.clone()).await;
        // Store broadcaster for status event emission
        *self.inner.broadcaster.lock().await = Some(broadcaster.clone());

        super::runtime_status::spawn_runtime_status_listener(
            self.take_runtime_receiver().await,
            self.clone_ref(),
            db_conn.clone(),
        );

        let db_conn2 = db_conn.clone();

        // Create shared session bridge
        let bridge = Arc::new(Mutex::new(SessionBridge::new()));

        // Spawn event subscriber
        let manager_for_events = self.clone_ref();
        super::event_subscriber::spawn_event_subscriber(
            bus.clone(),
            manager_for_events,
            db_conn.clone(),
            bridge.clone(),
        );

        // Spawn session event subscriber (ACP event routing to channels)
        let manager_for_session_events = self.clone_ref();
        super::session_event_subscriber::spawn_session_event_subscriber(
            bus.clone(),
            bridge.clone(),
            manager_for_session_events,
            conn_mgr.clone_ref(),
            db_conn.clone(),
        );

        // Spawn command dispatcher
        if let Some(command_rx) = self.take_command_receiver().await {
            tracing::info!("[ChatChannel] command dispatcher started");
            let manager_for_cmds = self.clone_ref();
            super::command_dispatcher::spawn_command_dispatcher(
                command_rx,
                manager_for_cmds,
                db_conn.clone(),
                data_dir,
                conn_mgr,
                emitter,
                bridge,
            );
        } else {
            tracing::warn!(
                "[ChatChannel] WARNING: command_rx already taken, dispatcher NOT started"
            );
        }

        // Spawn daily report scheduler
        let manager_for_scheduler = self.clone_ref();
        super::scheduler::spawn_daily_report_scheduler(manager_for_scheduler, db_conn.clone());

        // Reconcile only user- or Agent-created channels. New installations
        // intentionally start with an empty channel list.
        super::reconcile::reconcile_all_enabled(&self.clone_ref(), &db_conn2, "app_start").await;
    }
}

fn connection_status_name(status: ChannelConnectionStatus) -> &'static str {
    match status {
        ChannelConnectionStatus::Connected => "connected",
        ChannelConnectionStatus::Connecting => "connecting",
        ChannelConnectionStatus::Disconnected => "disconnected",
        ChannelConnectionStatus::Error => "error",
    }
}
