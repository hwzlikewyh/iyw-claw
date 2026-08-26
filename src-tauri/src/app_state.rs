use std::path::PathBuf;
use std::sync::Arc;

use crate::acp::delegation::broker::DelegationBroker;
use crate::acp::delegation::listener::TokenRegistry;
use crate::acp::manager::ConnectionManager;
use crate::acp::InternalEventBus;
use crate::chat_channel::manager::ChatChannelManager;
use crate::db::AppDatabase;
use crate::terminal::manager::TerminalManager;
use crate::web::event_bridge::{EventEmitter, WebEventBroadcaster};
use crate::web::WebServerState;
use crate::workspace_transfer::WorkspaceTransferManager;

pub struct AppState {
    pub db: AppDatabase,
    pub agent_catalog: crate::acp::version_center::CatalogStore,
    pub capability_policy: crate::acp::capability_policy::CapabilityPolicyStore,
    pub capability_policy_refresh:
        Arc<crate::acp::capability_policy::CapabilityPolicyRefreshRuntime>,
    pub plugin_registry: crate::plugin_runtime::registry::PluginRegistry,
    pub connection_manager: ConnectionManager,
    pub terminal_manager: TerminalManager,
    pub event_broadcaster: Arc<WebEventBroadcaster>,
    /// Process-wide bus for typed `Arc<EventEnvelope>` delivery to
    /// in-process consumers (lifecycle, pet state mapper, chat-channel
    /// subscribers). Distinct from `event_broadcaster`, which carries
    /// JSON-shaped `WebEvent`s for transport-bound delivery.
    pub acp_event_bus: Arc<InternalEventBus>,
    pub emitter: EventEmitter,
    pub data_dir: PathBuf,
    pub web_server_state: WebServerState,
    pub chat_channel_manager: ChatChannelManager,
    pub workspace_transfer: Arc<WorkspaceTransferManager>,
    /// Multi-agent delegation broker used by the main-process HTTP MCP
    /// dispatcher in both desktop and server mode. The settings UI hot-swaps
    /// its live configuration through `delegation_broker.set_config`.
    pub delegation_broker: Arc<DelegationBroker>,
    /// Per-connection broker credentials owned by the HTTP MCP lease manager
    /// and revoked with the parent ACP connection.
    pub delegation_tokens: Arc<TokenRegistry>,
    /// Hot-swappable live-feedback (`check_user_feedback`) enable flag. Shared
    /// with the `DelegationInjection` so MCP injection reads it, and updated by
    /// the feedback settings command on save. Populated at startup by
    /// `apply_persisted_feedback_config`.
    pub feedback_config: crate::acp::feedback::FeedbackRuntimeConfig,
    /// Hot-swappable ask-user-question (`ask_user_question`) enable flag. Shared
    /// with the `DelegationInjection` so MCP injection reads it, and updated by
    /// the question settings command on save. Populated at startup by
    /// `apply_persisted_question_config`.
    pub question_config: crate::acp::question::QuestionRuntimeConfig,
    /// Hot-swappable get-session-info (`get_session_info`) enable flag. Shared
    /// with the `DelegationInjection` so MCP injection reads it, and updated by
    /// the session-info settings command on save. Populated at startup by
    /// `apply_persisted_session_info_config`.
    pub session_info_config: crate::acp::session_info::SessionInfoRuntimeConfig,
    /// Canonical backend-owned user memory/profile/soul store. The settings
    /// API, launch-context snapshots, and Agent append tool all share it.
    pub user_memory: Arc<crate::user_memory::UserMemoryService>,
    /// Serializes mutually-exclusive system operations — in-place
    /// self-update, restart, rollback — so a second click can't race a
    /// download/swap already in flight. Handlers `try_lock` and reject when
    /// held (an upgrade is already running).
    pub system_op_lock: Arc<tokio::sync::Mutex<()>>,
    /// Source of truth for an in-flight / completed app self-update, shared by
    /// the desktop (tauri-plugin-updater) and server (in-place swap) paths.
    /// The upgrade UI subscribes to it and re-syncs from a snapshot on mount,
    /// so download progress survives settings-page navigation and reloads.
    pub update_state: crate::update::AppUpdateStateHandle,
}

pub fn default_system_op_lock() -> Arc<tokio::sync::Mutex<()>> {
    Arc::new(tokio::sync::Mutex::new(()))
}

pub fn default_update_state() -> crate::update::AppUpdateStateHandle {
    crate::update::new_update_state_handle()
}

pub fn default_connection_manager() -> ConnectionManager {
    ConnectionManager::new()
}

pub fn default_terminal_manager() -> TerminalManager {
    TerminalManager::new()
}

pub fn default_chat_channel_manager() -> ChatChannelManager {
    ChatChannelManager::new()
}

pub async fn build_capability_policy_stack(
    conn: sea_orm::DatabaseConnection,
) -> (
    crate::acp::capability_policy::CapabilityPolicyStore,
    Arc<crate::acp::capability_policy::CapabilityPolicyRefreshRuntime>,
) {
    use crate::acp::capability_policy::{
        install_runtime_enforcer, AppMetadataPolicyCache, CapabilityEnforcer,
        CapabilityPolicyRefreshRuntime, CapabilityPolicyStore, RefreshConfig,
        SnapshotValidationRules,
    };
    use crate::acp::version_center::CapabilityPolicyHttpFetcher;

    let cache = Arc::new(AppMetadataPolicyCache::new(conn.clone()));
    let store = CapabilityPolicyStore::new(cache, SnapshotValidationRules::default());
    if let Err(error) = store.restore_cache().await {
        tracing::warn!(
            error = %error,
            "[capability-policy] ignored invalid persisted snapshot"
        );
    }
    install_runtime_enforcer(CapabilityEnforcer::new(conn.clone(), store.clone()));
    let fetcher = Arc::new(CapabilityPolicyHttpFetcher::new(conn));
    let runtime = Arc::new(CapabilityPolicyRefreshRuntime::start(
        store.clone(),
        fetcher,
        RefreshConfig::default(),
    ));
    (store, runtime)
}

/// Build the delegation broker and token registry shared by the desktop and
/// server main-process HTTP MCP services.
pub fn build_delegation_stack(
    connection_manager: &ConnectionManager,
    db_conn: sea_orm::DatabaseConnection,
    data_dir: PathBuf,
) -> (
    Arc<DelegationBroker>,
    Arc<TokenRegistry>,
    crate::acp::feedback::FeedbackRuntimeConfig,
    crate::acp::question::QuestionRuntimeConfig,
    crate::acp::session_info::SessionInfoRuntimeConfig,
) {
    use crate::acp::connection::DelegationInjection;
    use crate::acp::delegation::broker::{
        ChildStatusLookup, ConversationDepthLookup, DbChildStatusLookup, DbDepthLookup,
    };
    use crate::acp::delegation::event_emitter::{
        ConnectionManagerEventEmitter, DelegationEventEmitter,
    };
    use crate::acp::delegation::live_reply::{
        ChildLiveReplyLookup, ConnectionManagerLiveReplyLookup,
    };
    use crate::acp::delegation::meta_writer::{ConnectionManagerMetaWriter, DelegationMetaWriter};
    use crate::acp::delegation::spawner::ConnectionSpawner;
    use crate::acp::manager::ConnectionManagerSpawner;

    let cm_arc = Arc::new(connection_manager.clone_ref());
    let db_arc = Arc::new(AppDatabase {
        conn: db_conn.clone(),
    });
    let spawner = Arc::new(ConnectionManagerSpawner {
        manager: cm_arc.clone(),
        db: db_arc.clone(),
        data_dir: Arc::new(data_dir),
    }) as Arc<dyn ConnectionSpawner>;
    let depth_lookup =
        Arc::new(DbDepthLookup { db: db_arc.clone() }) as Arc<dyn ConversationDepthLookup>;
    let status_lookup = Arc::new(DbChildStatusLookup { db: db_arc }) as Arc<dyn ChildStatusLookup>;
    let meta_writer = Arc::new(ConnectionManagerMetaWriter {
        manager: cm_arc.clone(),
    }) as Arc<dyn DelegationMetaWriter>;
    let live_reply_lookup = Arc::new(ConnectionManagerLiveReplyLookup {
        manager: cm_arc.clone(),
    }) as Arc<dyn ChildLiveReplyLookup>;
    let event_emitter = Arc::new(ConnectionManagerEventEmitter { manager: cm_arc })
        as Arc<dyn DelegationEventEmitter>;
    let broker = Arc::new(
        DelegationBroker::with_writers(spawner, depth_lookup, meta_writer, event_emitter)
            .with_status_lookup(status_lookup)
            .with_live_reply_lookup(live_reply_lookup),
    );
    let tokens = Arc::new(TokenRegistry::default());
    let feedback = crate::acp::feedback::FeedbackRuntimeConfig::new();
    let ask = crate::acp::question::QuestionRuntimeConfig::new();
    let sessions = crate::acp::session_info::SessionInfoRuntimeConfig::new();
    let confirmations = Arc::new(crate::acp::ConnectionManagerChannelConfirmationLookup {
        manager: Arc::new(connection_manager.clone_ref()),
    });

    // Install the injection on the manager so spawn_agent picks it up
    // without an extra parameter at every call site.
    connection_manager.install_delegation(DelegationInjection {
        broker: broker.clone(),
        tokens: tokens.clone(),
        feedback: feedback.clone(),
        ask: ask.clone(),
        sessions: sessions.clone(),
        // Same backing manager as the listener's question lookup; used only by
        // the run_connection teardown guard to reclaim a parked ask.
        questions: Arc::new(crate::acp::manager::ConnectionManagerQuestionLookup {
            manager: Arc::new(connection_manager.clone_ref()),
        }) as Arc<dyn crate::acp::question::SessionQuestionAccess>,
        confirmations,
    });

    (broker, tokens, feedback, ask, sessions)
}
