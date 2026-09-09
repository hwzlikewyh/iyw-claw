use std::sync::Arc;
use std::sync::Weak;
use std::time::Duration;

use codex_analytics::AnalyticsEventsClient;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ThreadGoal;
use codex_app_server_protocol::ThreadGoalUpdatedNotification;
use codex_app_server_protocol::ThreadQueueChangedNotification;
use codex_app_server_protocol::WarningNotification;
use codex_core::NewThread;
use codex_core::StartThreadOptions;
use codex_core::ThreadManager;
use codex_core::config::Config;
use codex_exec_server::EnvironmentManager;
use codex_extension_api::AgentSpawnFuture;
use codex_extension_api::AgentSpawner;
use codex_extension_api::ExtensionEventSink;
use codex_extension_api::ExtensionRegistry;
use codex_extension_api::ExtensionRegistryBuilder;
use codex_extension_api::ExtensionWarning;
use codex_extension_api::InternalSessionSpawnFuture;
use codex_extension_api::InternalSessionSpawner;
use codex_goal_extension::GoalExtensionConfig;
use codex_goal_extension::GoalService;
use codex_http_client::HttpClientFactory;
use codex_login::AuthManager;
use codex_protocol::ThreadId;
use codex_protocol::error::CodexErr;
use codex_protocol::protocol::Event;
use codex_protocol::protocol::EventMsg;
use codex_queue_extension::QueuedItemService;
use codex_rollout::state_db::StateDbHandle;

use crate::outgoing_message::OutgoingMessageSender;
use crate::outgoing_message::ThreadScopedOutgoingMessageSender;
use crate::thread_state::ThreadListenerCommand;
use crate::thread_state::ThreadStateManager;

pub(crate) struct ThreadExtensionDependencies {
    pub(crate) event_sink: Arc<dyn ExtensionEventSink>,
    pub(crate) auth_manager: Arc<AuthManager>,
    pub(crate) state_db: Option<StateDbHandle>,
    pub(crate) analytics_events_client: AnalyticsEventsClient,
    pub(crate) thread_manager: Weak<ThreadManager>,
    pub(crate) goal_service: Arc<GoalService>,
    pub(crate) environment_manager: Arc<EnvironmentManager>,
    pub(crate) executor_skill_provider: Arc<dyn codex_skills_extension::SkillProvider>,
    pub(crate) git_attribution_base_url: String,
    pub(crate) http_client_factory: HttpClientFactory,
    /// Process-scoped queue shared by idle dispatch and app-server requests.
    pub(crate) queue_service: Option<Arc<QueuedItemService>>,
}

pub(crate) fn thread_extensions<S>(
    guardian_agent_spawner: S,
    dependencies: ThreadExtensionDependencies,
) -> Arc<ExtensionRegistry<Config>>
where
    S: AgentSpawner<StartThreadOptions, Spawned = NewThread, Error = CodexErr> + 'static,
{
    let ThreadExtensionDependencies {
        event_sink,
        auth_manager,
        state_db,
        analytics_events_client,
        thread_manager,
        goal_service,
        environment_manager,
        executor_skill_provider,
        git_attribution_base_url,
        http_client_factory,
        queue_service,
    } = dependencies;
    let mut builder = ExtensionRegistryBuilder::<Config>::with_event_sink(Arc::clone(&event_sink));
    if let Some(queue_service) = queue_service {
        codex_queue_extension::install(&mut builder, queue_service);
    }
    codex_history_notes_extension::install(&mut builder, auth_manager.clone());
    if let Some(state_db) = state_db {
        codex_goal_extension::install_with_backend(
            &mut builder,
            state_db,
            analytics_events_client,
            codex_otel::global(),
            thread_manager.clone(),
            goal_service,
            |config: &Config| GoalExtensionConfig {
                enabled: config.features.enabled(codex_features::Feature::Goals),
                max_goal_token_budget: config.max_goal_token_budget,
            },
        );
    }
    codex_git_attribution::install(
        &mut builder,
        auth_manager.clone(),
        git_attribution_base_url,
        http_client_factory,
    );
    codex_guardian_v2::install(
        &mut builder,
        guardian_agent_spawner,
        internal_session_spawner(thread_manager.clone()),
        auth_manager.clone(),
        thread_manager,
    );
    codex_memories_extension::install(&mut builder, codex_otel::global());
    codex_mcp_extension::install(&mut builder);
    codex_mcp_extension::install_executor_plugins(&mut builder, environment_manager);
    codex_web_search_extension::install(&mut builder, auth_manager.clone());
    codex_image_generation_extension::install(&mut builder, auth_manager, |config: &Config| {
        Some(config.codex_home.clone())
    });
    let skill_providers = codex_skills_extension::SkillProviders::new()
        .with_executor_provider(executor_skill_provider)
        .with_orchestrator_provider(Arc::new(
            codex_skills_extension::OrchestratorSkillProvider::new(),
        ))
        .with_host_provider(Arc::new(codex_skills_extension::HostSkillProvider::new()));
    codex_skills_extension::install_with_providers_and_metrics(
        &mut builder,
        skill_providers,
        codex_otel::global(),
        |config: &Config| codex_skills_extension::SkillsExtensionConfig {
            include_instructions: config.include_skill_instructions,
            max_context_tokens: config.skill_max_context_tokens,
            bundled_skills_enabled: config.bundled_skills_enabled(),
            orchestrator_skills_enabled: config.orchestrator_skills_enabled,
            shadow_selection_enabled: config
                .features
                .enabled(codex_features::Feature::SkillSearch),
        },
    );
    Arc::new(builder.build())
}

pub(crate) fn app_server_extension_event_sink(
    outgoing: Arc<OutgoingMessageSender>,
    thread_state_manager: ThreadStateManager,
) -> Arc<dyn ExtensionEventSink> {
    Arc::new(AppServerExtensionEventSink {
        outgoing,
        thread_state_manager,
    })
}

pub(crate) async fn send_thread_warning(
    outgoing: &Arc<OutgoingMessageSender>,
    thread_state_manager: &ThreadStateManager,
    thread_id: ThreadId,
    message: String,
) {
    let subscribed_connection_ids = thread_state_manager
        .subscribed_connection_ids(thread_id)
        .await;
    let thread_outgoing = ThreadScopedOutgoingMessageSender::new(
        Arc::clone(outgoing),
        subscribed_connection_ids,
        thread_id,
    );
    thread_outgoing
        .send_server_notification(ServerNotification::Warning(WarningNotification {
            thread_id: Some(thread_id.to_string()),
            message,
        }))
        .await;
}

struct AppServerExtensionEventSink {
    outgoing: Arc<OutgoingMessageSender>,
    thread_state_manager: ThreadStateManager,
}

const MAX_EXTENSION_WARNING_BYTES: usize = 256;
const EXTENSION_WARNING_SUBSCRIBER_TIMEOUT: Duration = Duration::from_secs(10);

impl ExtensionEventSink for AppServerExtensionEventSink {
    fn emit(&self, event: Event) {
        match event.msg {
            EventMsg::ThreadQueueChanged(queue_event) => {
                let thread_id = queue_event.thread_id;
                if let Some(listener_command_tx) = self
                    .thread_state_manager
                    .current_listener_command_tx(thread_id)
                {
                    let command = ThreadListenerCommand::EmitThreadQueueChanged;
                    if listener_command_tx.send(command).is_ok() {
                        return;
                    }
                    tracing::warn!(
                        "failed to enqueue extension queue update for {thread_id}: listener command channel is closed"
                    );
                }
                let outgoing = Arc::clone(&self.outgoing);
                let thread_state_manager = self.thread_state_manager.clone();
                tokio::spawn(async move {
                    let subscribed_connection_ids = thread_state_manager
                        .subscribed_connection_ids(thread_id)
                        .await;
                    let outgoing = ThreadScopedOutgoingMessageSender::new(
                        outgoing,
                        subscribed_connection_ids,
                        thread_id,
                    );
                    outgoing
                        .send_server_notification(ServerNotification::ThreadQueueChanged(
                            ThreadQueueChangedNotification {
                                thread_id: thread_id.to_string(),
                            },
                        ))
                        .await;
                });
            }
            EventMsg::ThreadGoalUpdated(thread_goal_event) => {
                let thread_id = thread_goal_event.thread_id;
                let turn_id = thread_goal_event.turn_id;
                let goal: ThreadGoal = thread_goal_event.goal.into();
                if let Some(listener_command_tx) = self
                    .thread_state_manager
                    .current_listener_command_tx(thread_id)
                {
                    let command = ThreadListenerCommand::EmitThreadGoalUpdated {
                        turn_id: turn_id.clone(),
                        goal: goal.clone(),
                    };
                    if listener_command_tx.send(command).is_ok() {
                        return;
                    }
                    tracing::warn!(
                        "failed to enqueue extension goal update for {thread_id}: listener command channel is closed"
                    );
                }
                let outgoing = Arc::clone(&self.outgoing);
                tokio::spawn(async move {
                    outgoing
                        .send_server_notification(ServerNotification::ThreadGoalUpdated(
                            ThreadGoalUpdatedNotification {
                                thread_id: thread_id.to_string(),
                                turn_id,
                                goal,
                            },
                        ))
                        .await;
                });
            }
            msg => {
                tracing::debug!(event_id = %event.id, ?msg, "dropping unsupported extension event");
            }
        }
    }

    fn emit_warning(&self, warning: ExtensionWarning) {
        let ExtensionWarning {
            thread_id,
            turn_id: _,
            message,
        } = warning;
        let Ok(thread_id) = ThreadId::from_string(&thread_id) else {
            tracing::warn!(
                %thread_id,
                "dropping extension warning with invalid thread id"
            );
            return;
        };
        let mut message = message;
        if message.len() > MAX_EXTENSION_WARNING_BYTES {
            let mut truncate_at = MAX_EXTENSION_WARNING_BYTES;
            while !message.is_char_boundary(truncate_at) {
                truncate_at -= 1;
            }
            message.truncate(truncate_at);
        }
        if let Some(listener_command_tx) = self
            .thread_state_manager
            .current_listener_command_tx(thread_id)
        {
            let command = ThreadListenerCommand::EmitWarning {
                message: message.clone(),
            };
            if listener_command_tx.send(command).is_ok() {
                return;
            }
            tracing::warn!(
                "failed to enqueue extension warning for {thread_id}: listener command channel is closed"
            );
        }
        let outgoing = Arc::clone(&self.outgoing);
        let thread_state_manager = self.thread_state_manager.clone();
        tokio::spawn(async move {
            if tokio::time::timeout(
                EXTENSION_WARNING_SUBSCRIBER_TIMEOUT,
                thread_state_manager.wait_for_thread_subscriber(thread_id),
            )
            .await
            .is_err()
            {
                tracing::warn!(
                    %thread_id,
                    timeout_secs = EXTENSION_WARNING_SUBSCRIBER_TIMEOUT.as_secs(),
                    "dropping extension warning after waiting for a thread subscriber"
                );
                return;
            }
            send_thread_warning(&outgoing, &thread_state_manager, thread_id, message).await;
        });
    }
}

pub(crate) fn guardian_agent_spawner(
    thread_manager: Weak<ThreadManager>,
) -> impl AgentSpawner<StartThreadOptions, Spawned = NewThread, Error = CodexErr> {
    move |forked_from_thread_id: ThreadId,
          options: StartThreadOptions|
          -> AgentSpawnFuture<'static, NewThread, CodexErr> {
        let thread_manager = thread_manager.clone();
        Box::pin(async move {
            let thread_manager = thread_manager.upgrade().ok_or_else(|| {
                CodexErr::UnsupportedOperation("thread manager dropped".to_string())
            })?;
            thread_manager
                .spawn_subagent(forked_from_thread_id, options)
                .await
        })
    }
}

fn internal_session_spawner(
    thread_manager: Weak<ThreadManager>,
) -> impl InternalSessionSpawner<StartThreadOptions, Spawned = NewThread, Error = CodexErr> {
    move |parent_thread_id: ThreadId,
          options: StartThreadOptions|
          -> InternalSessionSpawnFuture<'static, NewThread, CodexErr> {
        let thread_manager = thread_manager.clone();
        Box::pin(async move {
            let thread_manager = thread_manager.upgrade().ok_or_else(|| {
                CodexErr::UnsupportedOperation("thread manager dropped".to_string())
            })?;
            thread_manager
                .spawn_internal_session(parent_thread_id, options)
                .await
        })
    }
}
