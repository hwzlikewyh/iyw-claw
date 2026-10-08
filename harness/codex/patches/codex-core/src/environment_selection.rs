use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt;
use std::future::Future;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::PoisonError;

use async_channel::Sender;
use codex_exec_server::Environment;
use codex_exec_server::EnvironmentConnectionState;
use codex_exec_server::EnvironmentInfo;
use codex_exec_server::EnvironmentManager;
use codex_exec_server::ExecServerError;
use codex_exec_server::ExecutorFileSystem;
use codex_exec_server::SelectedCapabilityRootsStatus;
use codex_protocol::capabilities::CapabilityRootLocation;
use codex_protocol::capabilities::SelectedCapabilityRoot;
use codex_protocol::error::CodexErr;
use codex_protocol::error::Result as CodexResult;
use codex_protocol::models::PermissionProfile;
use codex_protocol::protocol::AskForApproval;
use codex_protocol::protocol::EnvironmentConfig;
use codex_protocol::protocol::EnvironmentConfigState;
use codex_protocol::protocol::EnvironmentConnectionEvent;
use codex_protocol::protocol::Event;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::TurnEnvironmentSelection;
use codex_utils_absolute_path::AbsolutePathBuf;
use codex_utils_path_uri::PathUri;
use futures::FutureExt;
use futures::future::BoxFuture;
use futures::future::Shared;
use tokio::sync::watch;
use tokio_util::task::AbortOnDropHandle;
use tracing::Instrument;
use tracing::instrument::WithSubscriber;

use crate::session::ThreadEnvironmentDefaults;
use crate::session::turn_context::ShellSnapshotCache;
use crate::session::turn_context::ShellSnapshotTask;
use crate::session::turn_context::TurnEnvironment;
use crate::shell::Shell;
use crate::shell_snapshot::ShellSnapshot;
use crate::shell_snapshot::SnapshotCredentialBrokerState;

// Reject pathological selected cwd values at the environment-selection boundary.
const MAX_TURN_ENVIRONMENT_CWD_BYTES: usize = 8 * 1024;

/// Records whether a normalized config should follow later thread setting updates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EnvironmentConfigOrigin {
    Thread,
    Owner,
}

impl EnvironmentConfigOrigin {
    /// Reconstructs the input form so another attachment boundary preserves config ownership.
    pub(crate) fn into_input_selection(
        self,
        mut selection: TurnEnvironmentSelection,
    ) -> TurnEnvironmentSelection {
        if self == Self::Thread {
            selection.config = EnvironmentConfigState::FromThread;
        }
        selection
    }

    pub(crate) fn selected_capability_roots(
        self,
        environment: &Environment,
        config: &EnvironmentConfig,
    ) -> Vec<SelectedCapabilityRoot> {
        match self {
            Self::Thread => environment.selected_capability_roots(),
            Self::Owner => config.selected_capability_roots.clone(),
        }
    }
}

/// Combines persisted thread roots with attachment roots in selection order.
///
/// Thread-owned attachments still rely on roots reported by legacy executors. A matching live root
/// refreshes the persisted location while explicit owner configuration retains the existing
/// thread-first collision behavior.
pub(crate) fn combine_selected_capability_roots(
    thread_roots: &[SelectedCapabilityRoot],
    sources: impl IntoIterator<Item = (EnvironmentConfigOrigin, Vec<SelectedCapabilityRoot>)>,
) -> Vec<SelectedCapabilityRoot> {
    let attachment_roots = sources
        .into_iter()
        .flat_map(|(config_origin, roots)| roots.into_iter().map(move |root| (config_origin, root)))
        .collect::<Vec<_>>();
    let mut combined_roots = thread_roots
        .iter()
        .map(|thread_root| {
            let CapabilityRootLocation::Environment {
                environment_id: thread_environment_id,
                ..
            } = &thread_root.location;
            attachment_roots
                .iter()
                .find_map(|(origin, attachment_root)| {
                    if *origin != EnvironmentConfigOrigin::Thread
                        || attachment_root.id != thread_root.id
                    {
                        return None;
                    }
                    let CapabilityRootLocation::Environment {
                        environment_id: attachment_environment_id,
                        ..
                    } = &attachment_root.location;
                    (attachment_environment_id == thread_environment_id).then_some(attachment_root)
                })
                .unwrap_or(thread_root)
                .clone()
        })
        .collect::<Vec<_>>();
    combined_roots.extend(attachment_roots.into_iter().map(|(_, root)| root));
    combined_roots
}

pub(crate) fn default_thread_environment_selections(
    environment_manager: &EnvironmentManager,
    cwd: &AbsolutePathBuf,
    workspace_roots: &[AbsolutePathBuf],
) -> Vec<TurnEnvironmentSelection> {
    environment_manager
        .default_environment_ids()
        .into_iter()
        .map(|environment_id| TurnEnvironmentSelection {
            environment_id,
            cwd: PathUri::from_abs_path(cwd),
            workspace_roots: workspace_roots.iter().map(PathUri::from_abs_path).collect(),
            config: EnvironmentConfigState::FromThread,
        })
        .collect()
}

/// Checks that environment IDs are registered and unique and that working directories are not too long.
pub fn validate_environment_ids_and_cwds(
    environment_manager: &EnvironmentManager,
    environments: &[TurnEnvironmentSelection],
) -> CodexResult<()> {
    let mut environment_ids = HashSet::with_capacity(environments.len());
    for environment in environments {
        if environment.cwd.inferred_native_path_string().len() > MAX_TURN_ENVIRONMENT_CWD_BYTES {
            return Err(CodexErr::InvalidRequest(
                "turn environment working directory exceeds the maximum size".to_string(),
            ));
        }
        if !environment_ids.insert(environment.environment_id.as_str()) {
            return Err(CodexErr::InvalidRequest(format!(
                "duplicate turn environment id `{}`",
                environment.environment_id
            )));
        }
        environment_manager
            .get_environment(&environment.environment_id)
            .ok_or_else(|| {
                CodexErr::InvalidRequest(format!(
                    "unknown turn environment id `{}`",
                    environment.environment_id
                ))
            })?;
    }
    Ok(())
}

type TurnEnvironmentResult = Result<ResolvedEnvironment, Arc<ExecServerError>>;
type TurnEnvironmentResolution = Shared<BoxFuture<'static, TurnEnvironmentResult>>;
type PendingConfigurationResult = Result<EnvironmentConfig, String>;
pub(crate) type PendingConfiguration = Shared<BoxFuture<'static, PendingConfigurationResult>>;

// Shared startup result used to build each turn's environment with its own config
// without restarting the connection or shell resolution.
#[derive(Clone)]
struct ResolvedEnvironment {
    environment: Arc<Environment>,
    shell: Option<Shell>,
    user_home_dir: Option<PathUri>,
    executor_platform_os: Option<String>,
    temporary_directories: Option<Vec<PathUri>>,
    shell_snapshot: ShellSnapshotTask,
    shell_snapshot_builder: ShellSnapshot,
    shell_snapshot_cache: ShellSnapshotCache,
    shell_snapshot_v2_supported: bool,
    installed_config: Option<EnvironmentConfig>,
}

#[derive(Clone)]
struct SelectedTurnEnvironment {
    selection: TurnEnvironmentSelection,
    config_origin: EnvironmentConfigOrigin,
    environment: Arc<Environment>,
    // Selection clones share one listener; the final handle drop aborts it.
    connection_events_task: Option<Arc<AbortOnDropHandle<()>>>,
    resolution: TurnEnvironmentResolution,
    // Executor retries keep this session's wait for its first accepted configuration.
    pending_completion: Option<watch::Sender<Option<PendingConfigurationResult>>>,
    // Descendants can await the original attachment's first accepted config or failure.
    owner_config_result: Option<PendingConfiguration>,
}

#[derive(Clone)]
pub(crate) struct StartingTurnEnvironment {
    pub(crate) selection: TurnEnvironmentSelection,
    config_origin: EnvironmentConfigOrigin,
    resolution: TurnEnvironmentResolution,
    environment: Arc<Environment>,
    owner_config_result: Option<PendingConfiguration>,
}

/// Uses the current thread defaults when requested and records who owns later config updates.
fn resolve_selection_config(
    mut selection: TurnEnvironmentSelection,
    defaults: &ThreadEnvironmentDefaults,
) -> (TurnEnvironmentSelection, EnvironmentConfigOrigin) {
    let (config, origin) = match selection.config {
        EnvironmentConfigState::FromThread => (
            EnvironmentConfigState::Ready(defaults.for_selection(&selection)),
            EnvironmentConfigOrigin::Thread,
        ),
        config @ (EnvironmentConfigState::Ready(_)
        | EnvironmentConfigState::Pending
        | EnvironmentConfigState::Failed(_)) => (config, EnvironmentConfigOrigin::Owner),
    };
    selection.config = config;
    (selection, origin)
}

impl SelectedTurnEnvironment {
    fn refresh_thread_config(&mut self, defaults: &ThreadEnvironmentDefaults) {
        if self.config_origin == EnvironmentConfigOrigin::Thread {
            self.selection.config =
                EnvironmentConfigState::Ready(defaults.for_selection(&self.selection));
        }
    }
}

impl fmt::Debug for StartingTurnEnvironment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StartingTurnEnvironment")
            .field("selection", &self.selection)
            .field("resolved", &self.resolution.peek().is_some())
            .finish_non_exhaustive()
    }
}

impl StartingTurnEnvironment {
    pub(crate) fn owner_configuration(&self) -> Option<PendingConfiguration> {
        if matches!(self.selection.config, EnvironmentConfigState::Pending) {
            self.owner_config_result.clone()
        } else {
            None
        }
    }

    #[tracing::instrument(
        name = "environments.wait_until_ready",
        skip_all,
        fields(environment_id = %self.selection.environment_id)
    )]
    pub(crate) async fn wait_until_ready(&self) -> Result<(), Arc<ExecServerError>> {
        self.resolution.clone().await.map(|_| ())
    }
}

pub(crate) struct ThreadEnvironments {
    environment_manager: Arc<EnvironmentManager>,
    local_shell: Shell,
    shell_snapshot: ShellSnapshot,
    non_blocking_snapshots: bool,
    state: Mutex<ThreadEnvironmentsState>,
    connection_event_tx: OnceLock<Sender<Event>>,
}

struct ThreadEnvironmentsState {
    thread_defaults: ThreadEnvironmentDefaults,
    environments: Vec<SelectedTurnEnvironment>,
}

impl ThreadEnvironments {
    pub(crate) fn new(
        environment_manager: Arc<EnvironmentManager>,
        local_shell: Shell,
        thread_defaults: ThreadEnvironmentDefaults,
        shell_snapshot: ShellSnapshot,
        current: TurnEnvironmentSnapshot,
        non_blocking_snapshots: bool,
    ) -> Self {
        let environments = current
            .environments
            .into_iter()
            .filter_map(|inherited| {
                let environment = match inherited {
                    TurnEnvironmentState::Ready(environment) => environment,
                    TurnEnvironmentState::Starting(inherited) => {
                        let (selection, config_origin) = resolve_selection_config(
                            inherited
                                .config_origin
                                .into_input_selection(inherited.selection.clone()),
                            &thread_defaults,
                        );
                        return Some(Self::start_environment(
                            selection,
                            config_origin,
                            Arc::clone(&inherited.environment),
                            inherited.owner_configuration(),
                            /*pending_completion*/ None,
                            &local_shell,
                            &shell_snapshot,
                        ));
                    }
                    TurnEnvironmentState::Failed { .. } => return None,
                };
                let selection = environment.selection;
                let config_origin = environment.config_origin;
                let selected_environment = Arc::clone(&environment.environment);
                let inherited_snapshot = if !selected_environment.is_remote()
                    && shell_snapshot.should_rebuild_inherited()
                {
                    futures::future::ready(None).boxed().shared()
                } else {
                    environment.shell_snapshot
                };
                let resolution: TurnEnvironmentResolution =
                    futures::future::ready(Ok(ResolvedEnvironment {
                        environment: environment.environment,
                        shell: environment.shell,
                        user_home_dir: environment.user_home_dir,
                        executor_platform_os: environment.executor_platform_os,
                        temporary_directories: environment.temporary_directories,
                        shell_snapshot: inherited_snapshot,
                        shell_snapshot_builder: shell_snapshot.clone(),
                        shell_snapshot_cache: Arc::default(),
                        shell_snapshot_v2_supported: environment.shell_snapshot_v2_supported
                            && (selected_environment.is_remote()
                                || !shell_snapshot.should_rebuild_inherited()),
                        installed_config: None,
                    }))
                    .boxed()
                    .shared();
                let mut inherited_environment = SelectedTurnEnvironment {
                    selection,
                    config_origin,
                    environment: selected_environment,
                    connection_events_task: None,
                    resolution,
                    pending_completion: None,
                    owner_config_result: None,
                };
                // Child threads re-infer thread-owned config while preserving owner config.
                inherited_environment.refresh_thread_config(&thread_defaults);
                Some(inherited_environment)
            })
            .collect();
        Self {
            environment_manager,
            local_shell,
            shell_snapshot,
            non_blocking_snapshots,
            state: Mutex::new(ThreadEnvironmentsState {
                thread_defaults,
                environments,
            }),
            connection_event_tx: OnceLock::new(),
        }
    }

    fn start_shell_snapshot_task(
        shell_snapshot: ShellSnapshot,
        environment: Arc<Environment>,
        cwd: PathUri,
        shell: Option<Shell>,
        config: &EnvironmentConfigState,
    ) -> ShellSnapshotTask {
        // Protected snapshots require the command's sandbox and are captured lazily.
        if shell_snapshot.should_rebuild_inherited() {
            return futures::future::ready(None).boxed().shared();
        }
        let EnvironmentConfigState::Ready(config) = config else {
            return futures::future::ready(None).boxed().shared();
        };
        let shell_snapshot = shell_snapshot
            .build(
                environment,
                cwd,
                shell,
                /*allow_login_shell*/ true,
                config.shell_environment_policy.clone(),
                /*sandbox*/ None,
            )
            .boxed()
            .shared();
        drop(tokio::spawn(
            shell_snapshot
                .clone()
                .in_current_span()
                .with_current_subscriber(),
        ));
        shell_snapshot
    }

    /// Updates the selected list before waking work that was waiting on the previous selection.
    pub(crate) fn update_selections(&self, environments: &[TurnEnvironmentSelection]) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let ThreadEnvironmentsState {
            thread_defaults,
            environments: current,
        } = &mut *state;
        let mut seen_environment_ids = HashSet::with_capacity(environments.len());
        let mut next = Vec::with_capacity(environments.len());
        let mut pending_completions = Vec::new();
        for selected_environment in environments {
            if !seen_environment_ids.insert(selected_environment.environment_id.as_str()) {
                continue;
            }
            let previous = current.iter().find(|environment| {
                let previous = &environment.selection;
                previous.environment_id == selected_environment.environment_id
                    && previous.cwd == selected_environment.cwd
                    && previous.workspace_roots == selected_environment.workspace_roots
            });
            let (selected_environment, config_origin) =
                resolve_selection_config(selected_environment.clone(), thread_defaults);
            if let Some(environment) = previous {
                let mut environment = environment.clone();
                if let Some(completion) = environment.pending_completion.take() {
                    match &selected_environment.config {
                        EnvironmentConfigState::Ready(config) => {
                            pending_completions.push((completion, Ok(config.clone())));
                        }
                        EnvironmentConfigState::Failed(error) => {
                            pending_completions.push((completion, Err(error.clone())));
                        }
                        EnvironmentConfigState::FromThread | EnvironmentConfigState::Pending => {
                            environment.pending_completion = Some(completion);
                        }
                    }
                }
                let failed =
                    matches!(
                        environment.selection.config,
                        EnvironmentConfigState::Failed(_)
                    ) || matches!(environment.resolution.clone().now_or_never(), Some(Err(_)));
                let restarting_as_pending =
                    matches!(selected_environment.config, EnvironmentConfigState::Pending)
                        && !matches!(
                            environment.selection.config,
                            EnvironmentConfigState::Pending
                        );

                if !failed && !restarting_as_pending {
                    let shell_settings_changed = matches!(
                        (&environment.selection.config, &selected_environment.config),
                        (EnvironmentConfigState::Ready(previous), EnvironmentConfigState::Ready(current))
                            if previous.allow_login_shell != current.allow_login_shell
                                || previous.shell_environment_policy != current.shell_environment_policy
                    );
                    environment.selection = selected_environment;
                    environment.config_origin = config_origin;
                    if shell_settings_changed
                        && !environment.environment.is_remote()
                        && self.shell_snapshot.should_rebuild_inherited()
                        && let Some(Ok(resolved)) = environment.resolution.clone().now_or_never()
                    {
                        self.restart_shell_snapshot(&mut environment, resolved);
                    }
                    next.push(environment);
                    continue;
                }
            }

            let environment_id = &selected_environment.environment_id;
            let Some(environment) = self.environment_manager.get_environment(environment_id) else {
                tracing::warn!("skipping unknown turn environment `{environment_id}`");
                continue;
            };
            // Connection state belongs to the environment instance, not its cwd or roots.
            let connection_events_task = current
                .iter()
                .find(|previous| {
                    previous.selection.environment_id.as_str() == environment_id.as_str()
                        && Arc::ptr_eq(&previous.environment, &environment)
                })
                .and_then(|previous| previous.connection_events_task.clone())
                .or_else(|| {
                    self.connection_event_tx.get().and_then(|tx_event| {
                        Self::spawn_connection_event_listener(
                            environment.as_ref(),
                            environment_id.clone(),
                            tx_event.clone(),
                        )
                    })
                });
            let pending = previous.filter(|previous| {
                matches!(selected_environment.config, EnvironmentConfigState::Pending)
                    && previous.pending_completion.is_some()
            });
            let mut selected = Self::start_environment(
                selected_environment,
                config_origin,
                environment,
                pending.and_then(|previous| previous.owner_config_result.clone()),
                pending.and_then(|previous| previous.pending_completion.clone()),
                &self.local_shell,
                &self.shell_snapshot,
            );
            selected.connection_events_task = connection_events_task;
            next.push(selected);
        }
        let removed_connection_tasks = current
            .iter()
            .filter_map(|previous| {
                let task = previous.connection_events_task.as_ref()?;
                (!next.iter().any(|next| {
                    next.connection_events_task
                        .as_ref()
                        .is_some_and(|next_task| Arc::ptr_eq(task, next_task))
                }))
                .then(|| Arc::clone(task))
            })
            .collect::<Vec<_>>();
        let previous = std::mem::replace(current, next);

        // Publish after installing the new list, including pending bindings replaced by retries.
        for (completion, result) in pending_completions {
            let _ = completion.send_replace(Some(result));
        }

        for task in removed_connection_tasks {
            task.abort();
        }

        drop(previous);
    }

    fn start_environment(
        selection: TurnEnvironmentSelection,
        config_origin: EnvironmentConfigOrigin,
        environment: Arc<Environment>,
        owner_config_result: Option<PendingConfiguration>,
        pending_completion: Option<watch::Sender<Option<PendingConfigurationResult>>>,
        local_shell: &Shell,
        shell_snapshot: &ShellSnapshot,
    ) -> SelectedTurnEnvironment {
        let (pending_completion, configuration_ready) =
            if matches!(selection.config, EnvironmentConfigState::Pending) {
                let sender =
                    pending_completion.unwrap_or_else(|| watch::channel(/*init*/ None).0);
                let mut receiver = sender.subscribe();
                let result = async move {
                    receiver
                        .wait_for(Option::is_some)
                        .await
                        .ok()
                        .and_then(|result| result.clone())
                        .unwrap_or_else(
                            || Err("environment configuration was canceled".to_string()),
                        )
                }
                .boxed()
                .shared();
                (Some(sender), Some(result))
            } else {
                (None, None)
            };
        // The resolver waits for this Session; descendants can follow the original owner.
        let owner_config_result = owner_config_result.or_else(|| configuration_ready.clone());
        let (task, resolution) = Self::resolve_environment(
            selection.clone(),
            Arc::clone(&environment),
            local_shell.clone(),
            shell_snapshot.clone(),
            configuration_ready,
        )
        .remote_handle();
        drop(tokio::spawn(
            task.in_current_span().with_current_subscriber(),
        ));
        SelectedTurnEnvironment {
            selection,
            config_origin,
            environment,
            connection_events_task: None,
            resolution: resolution.boxed().shared(),
            pending_completion,
            owner_config_result,
        }
    }

    /// Projects canonical selections back to caller input form without losing config ownership.
    pub(crate) fn selections(&self) -> Vec<TurnEnvironmentSelection> {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .environments
            .iter()
            .map(|environment| {
                environment
                    .config_origin
                    .into_input_selection(environment.selection.clone())
            })
            .collect()
    }

    /// Returns installed owner configuration without treating pending attachments as ready.
    pub(crate) fn primary_config_for(
        selections: &[TurnEnvironmentSelection],
    ) -> Option<&EnvironmentConfig> {
        match &selections.first()?.config {
            EnvironmentConfigState::Ready(config) => Some(config),
            EnvironmentConfigState::FromThread
            | EnvironmentConfigState::Pending
            | EnvironmentConfigState::Failed(_) => None,
        }
    }

    pub(crate) fn primary_workspace_roots_for(
        selections: &[TurnEnvironmentSelection],
    ) -> Vec<AbsolutePathBuf> {
        selections
            .first()
            .map(|selection| {
                selection
                    .workspace_roots
                    .iter()
                    .filter_map(|workspace_root| workspace_root.to_abs_path().ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Changes the defaults inherited by all selected and later-added environments. A step that
    /// already captured its environments keeps the previous values.
    pub(crate) fn set_active_thread_defaults(&self, defaults: EnvironmentConfig) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let ThreadEnvironmentsState {
            thread_defaults,
            environments,
        } = &mut *state;
        if thread_defaults.common == defaults {
            return;
        }
        thread_defaults.common = defaults;
        for environment in environments.iter_mut() {
            let shell_settings_changed = environment.config_origin
                == EnvironmentConfigOrigin::Thread
                && matches!(
                    &environment.selection.config,
                    EnvironmentConfigState::Ready(previous)
                        if previous.allow_login_shell != thread_defaults.common.allow_login_shell
                            || previous.shell_environment_policy != thread_defaults.common.shell_environment_policy
                );
            environment.refresh_thread_config(thread_defaults);
            if shell_settings_changed
                && !environment.environment.is_remote()
                && self.shell_snapshot.should_rebuild_inherited()
                && let Some(Ok(resolved)) = environment.resolution.clone().now_or_never()
            {
                self.restart_shell_snapshot(environment, resolved);
            }
        }
    }

    pub(crate) fn set_snapshot_credential_broker(&self, broker: SnapshotCredentialBrokerState) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if !self.shell_snapshot.set_credential_broker(broker) {
            return;
        }

        for selected in &mut state.environments {
            if !selected.environment.is_remote()
                && let Some(Ok(resolved)) = selected.resolution.clone().now_or_never()
            {
                self.restart_shell_snapshot(selected, resolved);
            }
        }
    }

    fn restart_shell_snapshot(
        &self,
        selected: &mut SelectedTurnEnvironment,
        resolved: ResolvedEnvironment,
    ) {
        let shell_snapshot = Self::start_shell_snapshot_task(
            self.shell_snapshot.clone(),
            Arc::clone(&resolved.environment),
            selected.selection.cwd.clone(),
            resolved.shell.clone(),
            &selected.selection.config,
        );
        selected.resolution = futures::future::ready(Ok(ResolvedEnvironment {
            shell_snapshot,
            shell_snapshot_cache: Arc::default(),
            shell_snapshot_v2_supported: cfg!(unix)
                && !self.shell_snapshot.should_rebuild_inherited(),
            ..resolved
        }))
        .boxed()
        .shared();
    }

    /// Combines persisted thread roots with installed attachment roots, keeping
    /// thread roots first and hiding attachments that are not ready yet.
    pub(crate) fn inspect_selected_capability_roots(
        &self,
        thread_selected_capability_roots: &[SelectedCapabilityRoot],
    ) -> SelectedCapabilityRootsStatus {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let environments = &state.environments;
        let mut selected_capability_roots = combine_selected_capability_roots(
            thread_selected_capability_roots,
            environments.iter().filter_map(|environment| {
                let EnvironmentConfigState::Ready(config) = &environment.selection.config else {
                    return None;
                };
                Some((
                    environment.config_origin,
                    environment
                        .config_origin
                        .selected_capability_roots(&environment.environment, config),
                ))
            }),
        );
        let mut seen_root_ids = HashSet::with_capacity(selected_capability_roots.len());
        selected_capability_roots.retain(|root| seen_root_ids.insert(root.id.clone()));

        let mut status = self
            .environment_manager
            .inspect_selected_capability_roots(&selected_capability_roots);
        status.ready_roots.retain(|root| {
            let CapabilityRootLocation::Environment { environment_id, .. } = &root.location;
            environments
                .iter()
                .find(|environment| &environment.selection.environment_id == environment_id)
                .is_none_or(|environment| {
                    matches!(environment.resolution.clone().now_or_never(), Some(Ok(_)))
                })
        });
        status
    }

    fn spawn_connection_event_listener(
        environment: &Environment,
        environment_id: String,
        tx_event: Sender<Event>,
    ) -> Option<Arc<AbortOnDropHandle<()>>> {
        let mut connection_state = environment.subscribe_connection_state()?;
        let task = tokio::spawn(async move {
            loop {
                let state = tokio::select! {
                    _ = tx_event.closed() => return,
                    changed = connection_state.changed() => {
                        if changed.is_err() {
                            return;
                        }
                        *connection_state.borrow_and_update()
                    }
                };
                let msg = match state {
                    EnvironmentConnectionState::Connected => {
                        EventMsg::EnvironmentConnected(EnvironmentConnectionEvent {
                            environment_id: environment_id.clone(),
                        })
                    }
                    EnvironmentConnectionState::Disconnected => {
                        EventMsg::EnvironmentDisconnected(EnvironmentConnectionEvent {
                            environment_id: environment_id.clone(),
                        })
                    }
                };
                if tx_event
                    .send(Event {
                        id: String::new(),
                        msg,
                    })
                    .await
                    .is_err()
                {
                    return;
                }
            }
        });
        Some(Arc::new(AbortOnDropHandle::new(task)))
    }

    pub(crate) fn start_connection_event_forwarding(&self, tx_event: Sender<Event>) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let tx_event = self.connection_event_tx.get_or_init(|| tx_event);
        for selected in &mut state.environments {
            if selected.connection_events_task.is_none() {
                selected.connection_events_task = Self::spawn_connection_event_listener(
                    selected.environment.as_ref(),
                    selected.selection.environment_id.clone(),
                    tx_event.clone(),
                );
            }
        }
    }

    #[tracing::instrument(
        name = "environments.resolve",
        skip_all,
        fields(
            environment_id = %selection.environment_id,
            remote = environment.is_remote(),
            configuration_pending = configuration_ready.is_some(),
        )
    )]
    async fn resolve_environment(
        selection: TurnEnvironmentSelection,
        environment: Arc<Environment>,
        local_shell: Shell,
        shell_snapshot: ShellSnapshot,
        configuration_ready: Option<PendingConfiguration>,
    ) -> TurnEnvironmentResult {
        let environment_id = &selection.environment_id;
        if let EnvironmentConfigState::Failed(error) = &selection.config {
            return Err(Arc::new(ExecServerError::Protocol(error.clone())));
        }

        // Wait for the shared executor connection.
        let connection_ready = async {
            environment.wait_until_ready().await.map_err(|error| {
                tracing::warn!("turn environment `{environment_id}` failed to start: {error}");
                Arc::new(error)
            })
        };
        // Wait for this thread to accept the configuration, if pending.
        let configuration_ready = async move {
            let Some(configuration_ready) = configuration_ready else {
                return Ok(None);
            };
            configuration_ready
                .await
                .map(Some)
                .map_err(|error| Arc::new(ExecServerError::Protocol(error)))
        };
        // Resolve the attachment only after both prerequisites are ready.
        let ((), installed_config) = tokio::try_join!(connection_ready, configuration_ready)?;
        let executor_platform_os;
        let (shell, user_home_dir, temporary_dirs, snapshot_v2) = if environment.is_remote() {
            match environment.info().await {
                Ok(info) => {
                    executor_platform_os = info.platform_os;
                    let user_home_dir = info.user_home_dir;
                    let temporary_directories = info.temporary_directories;
                    let shell_snapshot_v2_supported = info.capabilities.shell_snapshot_v2;
                    let shell = match Shell::from_environment_shell_info(info.shell) {
                        Ok(shell) => Some(shell),
                        Err(err) => {
                            tracing::warn!(
                                "failed to resolve shell for environment `{environment_id}`: {err}"
                            );
                            None
                        }
                    };
                    (
                        shell,
                        user_home_dir,
                        temporary_directories,
                        shell_snapshot_v2_supported,
                    )
                }
                Err(err) => {
                    executor_platform_os = None;
                    tracing::warn!("failed to get info for environment `{environment_id}`: {err}");
                    (None, None, None, false)
                }
            }
        } else {
            executor_platform_os = Some(std::env::consts::OS.to_string());
            (
                Some(local_shell),
                PathUri::from_host_native_path("~").ok(),
                Some(EnvironmentInfo::local_temporary_directories()),
                cfg!(unix),
            )
        };
        let shell_snapshot_builder = shell_snapshot.clone();
        let snapshot_config = installed_config
            .clone()
            .map(EnvironmentConfigState::Ready)
            .unwrap_or(selection.config);
        let task = Self::start_shell_snapshot_task(
            shell_snapshot,
            Arc::clone(&environment),
            selection.cwd,
            shell.clone(),
            &snapshot_config,
        );
        let shell_snapshot_v2_supported = snapshot_v2
            && (environment.is_remote() || !shell_snapshot_builder.should_rebuild_inherited());
        Ok(ResolvedEnvironment {
            environment,
            shell,
            user_home_dir,
            executor_platform_os,
            temporary_directories: temporary_dirs,
            shell_snapshot: task,
            shell_snapshot_builder,
            shell_snapshot_cache: Arc::default(),
            shell_snapshot_v2_supported,
            installed_config,
        })
    }

    /// Captures the selected list immediately, then returns a future that may wait for setup.
    /// This lets callers release their locks before waiting without accidentally using a newer
    /// selection. The trace still covers the setup wait itself.
    pub(crate) fn snapshot(
        &self,
    ) -> impl Future<Output = TurnEnvironmentSnapshot> + Send + 'static {
        self.resolve_snapshot(self.snapshot_now())
    }

    /// Copies the current selection without waiting for any executor to connect.
    pub(crate) fn snapshot_now(&self) -> TurnEnvironmentSnapshot {
        // Keep the startup results, but not the senders that would keep removed Pending work alive.
        let environments = self
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .environments
            .iter()
            .map(|environment| {
                let starting = StartingTurnEnvironment {
                    selection: environment.selection.clone(),
                    config_origin: environment.config_origin,
                    resolution: environment.resolution.clone(),
                    environment: Arc::clone(&environment.environment),
                    owner_config_result: environment.owner_config_result.clone(),
                };
                let resolved = starting.resolution.clone().now_or_never();
                TurnEnvironmentState::from_resolution(starting, resolved)
            })
            .collect();
        TurnEnvironmentSnapshot { environments }
    }

    /// Applies the same startup policy to an already captured selection without adopting newer ones.
    #[tracing::instrument(
        name = "environments.snapshot",
        skip_all,
        fields(
            environment_count = snapshot.environments.len(),
            non_blocking = non_blocking_snapshots,
        )
    )]
    fn resolve_snapshot(
        &self,
        snapshot: TurnEnvironmentSnapshot,
    ) -> impl Future<Output = TurnEnvironmentSnapshot> + Send + 'static {
        let non_blocking_snapshots = self.non_blocking_snapshots;
        async move {
            if !non_blocking_snapshots {
                for starting in snapshot.starting() {
                    if !matches!(starting.selection.config, EnvironmentConfigState::Pending) {
                        let _ = starting.wait_until_ready().await;
                    }
                }
            }
            // An earlier environment may have become ready while we waited for a later one.
            snapshot.refresh_readiness()
        }
    }

    pub(crate) fn environment_manager(&self) -> Arc<EnvironmentManager> {
        Arc::clone(&self.environment_manager)
    }
}

#[derive(Clone, Debug)]
pub(crate) enum TurnEnvironmentState {
    Ready(TurnEnvironment),
    Starting(StartingTurnEnvironment),
    /// Unavailable for execution, but still selected when evaluating permissions.
    Failed {
        // Keep the input form so connection failure doesn't hide whether config comes from the thread.
        selection: TurnEnvironmentSelection,
        error: String,
    },
}

impl TurnEnvironmentState {
    fn from_resolution(
        starting: StartingTurnEnvironment,
        resolved: Option<TurnEnvironmentResult>,
    ) -> Self {
        if let EnvironmentConfigState::Failed(error) = &starting.selection.config {
            let error = error.clone();
            return Self::Failed {
                selection: starting
                    .config_origin
                    .into_input_selection(starting.selection),
                error,
            };
        }
        match resolved {
            Some(Ok(environment)) => {
                let mut selection = starting.selection;
                if matches!(selection.config, EnvironmentConfigState::Pending) {
                    let Some(config) = environment.installed_config else {
                        return Self::Failed {
                            selection: starting.config_origin.into_input_selection(selection),
                            error: "Environment configuration was not supplied.".to_string(),
                        };
                    };
                    selection.config = EnvironmentConfigState::Ready(config);
                }
                let mut turn_environment = TurnEnvironment::new(
                    selection,
                    starting.config_origin,
                    environment.environment,
                    environment.shell,
                );
                turn_environment.executor_platform_os = environment.executor_platform_os;
                turn_environment.shell_snapshot = environment.shell_snapshot;
                turn_environment.shell_snapshot_builder =
                    Some(Box::new(environment.shell_snapshot_builder));
                turn_environment.shell_snapshot_cache = environment.shell_snapshot_cache;
                turn_environment.shell_snapshot_v2_supported =
                    environment.shell_snapshot_v2_supported;
                turn_environment.user_home_dir = environment.user_home_dir;
                turn_environment.temporary_directories = environment.temporary_directories;
                Self::Ready(turn_environment)
            }
            Some(Err(err)) => {
                tracing::debug!(
                    environment_id = %starting.selection.environment_id,
                    "skipping failed turn environment: {err}"
                );
                Self::Failed {
                    selection: starting
                        .config_origin
                        .into_input_selection(starting.selection),
                    error: err.to_string(),
                }
            }
            None => Self::Starting(starting),
        }
    }
}

/// Existing environment bindings captured for a turn. Internal child threads can
/// retain these bindings without resolving a different set of environments.
#[derive(Clone, Debug, Default)]
pub struct TurnEnvironmentSnapshot {
    // Keep every selected environment, including failures, in its original order.
    pub(crate) environments: Vec<TurnEnvironmentState>,
}

impl TurnEnvironmentSnapshot {
    pub(crate) fn has_full_access(
        &self,
        approval_policy: AskForApproval,
        thread_profile: &PermissionProfile,
    ) -> bool {
        codex_protocol::protocol::has_full_access(
            approval_policy,
            thread_profile,
            self.refresh_readiness()
                .environments
                .iter()
                .map(|environment| match environment {
                    TurnEnvironmentState::Ready(environment) => &environment.selection.config,
                    TurnEnvironmentState::Starting(_) | TurnEnvironmentState::Failed { .. } => {
                        &EnvironmentConfigState::Pending
                    }
                }),
        )
    }

    /// Promotes completed startup work without adopting newer thread selections.
    pub(crate) fn refresh_readiness(&self) -> Self {
        let environments = self
            .environments
            .iter()
            .map(|environment| match environment {
                TurnEnvironmentState::Ready(environment) => {
                    TurnEnvironmentState::Ready(environment.clone())
                }
                TurnEnvironmentState::Starting(environment) => {
                    TurnEnvironmentState::from_resolution(
                        environment.clone(),
                        environment.resolution.clone().now_or_never(),
                    )
                }
                TurnEnvironmentState::Failed { .. } => environment.clone(),
            })
            .collect();
        Self { environments }
    }

    pub(crate) fn turn_environments(&self) -> impl Iterator<Item = &TurnEnvironment> {
        self.environments.iter().filter_map(|environment| {
            let TurnEnvironmentState::Ready(environment) = environment else {
                return None;
            };
            Some(environment)
        })
    }

    pub(crate) fn starting(&self) -> impl Iterator<Item = &StartingTurnEnvironment> {
        self.environments.iter().filter_map(|environment| {
            let TurnEnvironmentState::Starting(environment) = environment else {
                return None;
            };
            Some(environment)
        })
    }

    pub(crate) fn ready_environment_handles(&self) -> HashMap<String, Arc<Environment>> {
        self.turn_environments()
            .map(|environment| {
                (
                    environment.selection.environment_id.clone(),
                    Arc::clone(&environment.environment),
                )
            })
            .collect()
    }

    /// Maps each captured environment to its exact ready handle, or `None` when it was starting.
    pub(crate) fn captured_environments(&self) -> HashMap<String, Option<Arc<Environment>>> {
        self.turn_environments()
            .map(|environment| {
                (
                    environment.selection.environment_id.clone(),
                    Some(Arc::clone(&environment.environment)),
                )
            })
            .chain(
                self.starting()
                    .map(|environment| (environment.selection.environment_id.clone(), None)),
            )
            .collect()
    }

    pub(crate) fn primary(&self) -> Option<&TurnEnvironment> {
        self.turn_environments().next()
    }

    /// Returns the first selected environment's host folders, even if setup is not ready yet.
    pub(crate) fn primary_workspace_roots(&self) -> Vec<AbsolutePathBuf> {
        self.primary_workspace_root_uris()
            .iter()
            .filter_map(|root| root.to_abs_path().ok())
            .collect()
    }

    /// Returns the first selection's executor paths, even when setup is not ready yet.
    pub(crate) fn primary_workspace_root_uris(&self) -> &[PathUri] {
        let selection = match self.environments.first() {
            Some(TurnEnvironmentState::Ready(environment)) => &environment.selection,
            Some(TurnEnvironmentState::Starting(environment)) => &environment.selection,
            Some(TurnEnvironmentState::Failed { selection, .. }) => selection,
            None => return &[],
        };
        &selection.workspace_roots
    }

    /// Returns the primary environment's resolved permissions, or the provided fallback.
    pub(crate) fn permission_profile_or_else(
        &self,
        fallback: impl FnOnce() -> PermissionProfile,
    ) -> PermissionProfile {
        self.primary()
            .map(TurnEnvironment::permission_profile_with_workspace_roots)
            .unwrap_or_else(fallback)
    }

    pub(crate) fn local(&self) -> Option<&TurnEnvironment> {
        self.turn_environments()
            .find(|environment| !environment.environment.is_remote())
    }

    pub(crate) fn local_environment_cwd(&self) -> Option<AbsolutePathBuf> {
        self.environments
            .iter()
            .find_map(|environment| match environment {
                TurnEnvironmentState::Ready(environment)
                    if !environment.environment.is_remote() =>
                {
                    environment.cwd().to_abs_path().ok()
                }
                TurnEnvironmentState::Ready(_) => None,
                TurnEnvironmentState::Starting(environment)
                    if environment.selection.environment_id
                        == codex_exec_server::LOCAL_ENVIRONMENT_ID =>
                {
                    environment.selection.cwd.to_abs_path().ok()
                }
                TurnEnvironmentState::Starting(_) | TurnEnvironmentState::Failed { .. } => None,
            })
    }


    pub(crate) fn to_selections(&self) -> Vec<TurnEnvironmentSelection> {
        self.turn_environments()
            .map(TurnEnvironment::selection)
            .collect()
    }

    /// Native children retain ready and starting attachments captured by their spawning step.
    pub(crate) fn inheritable_selections(&self) -> Vec<TurnEnvironmentSelection> {
        self.environments
            .iter()
            .filter_map(|environment| match environment {
                TurnEnvironmentState::Ready(environment) => Some(environment.selection()),
                TurnEnvironmentState::Starting(environment) => Some(
                    environment
                        .config_origin
                        .into_input_selection(environment.selection.clone()),
                ),
                TurnEnvironmentState::Failed { .. } => None,
            })
            .collect()
    }

    /// Returns every captured selection, including those still starting or unable to connect.
    pub(crate) fn all_selections(&self) -> Vec<TurnEnvironmentSelection> {
        self.environments
            .iter()
            .map(|environment| match environment {
                TurnEnvironmentState::Ready(environment) => environment.selection(),
                TurnEnvironmentState::Starting(environment) => environment
                    .config_origin
                    .into_input_selection(environment.selection.clone()),
                TurnEnvironmentState::Failed { selection, .. } => selection.clone(),
            })
            .collect()
    }

    pub(crate) fn primary_filesystem(&self) -> Option<Arc<dyn ExecutorFileSystem>> {
        self.primary()
            .map(|environment| environment.environment.get_filesystem())
    }

    pub(crate) fn single_local_environment(&self) -> Option<&TurnEnvironment> {
        if self.starting().next().is_some() {
            return None;
        }
        let mut environments = self.turn_environments();
        let environment = environments.next()?;
        if environments.next().is_some() {
            return None;
        }

        (!environment.environment.is_remote()).then_some(environment)
    }

    pub(crate) fn single_local_environment_cwd(&self) -> Option<AbsolutePathBuf> {
        // TODO(anp): Migrate local-environment consumers to PathUri so this compatibility
        // conversion can be removed.
        self.single_local_environment()?.cwd().to_abs_path().ok()
    }
}
