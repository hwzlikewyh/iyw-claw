use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt;
use std::sync::Arc;
use std::sync::OnceLock;

use arc_swap::ArcSwap;
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
use tokio::sync::mpsc;
use tokio_util::task::AbortOnDropHandle;
use tracing::Instrument;
use tracing::instrument::WithSubscriber;

use crate::session::turn_context::ShellSnapshotTask;
use crate::session::turn_context::TurnEnvironment;
use crate::shell::Shell;
use crate::shell_snapshot::ShellSnapshot;

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

type TurnEnvironmentResult = Result<ResolvedEnvironment, Arc<ExecServerError>>;
type TurnEnvironmentResolution = Shared<BoxFuture<'static, TurnEnvironmentResult>>;
type PendingConfigurationResult = Result<EnvironmentConfig, String>;

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
    pending_completion: Option<mpsc::Sender<PendingConfigurationResult>>,
}

#[derive(Clone)]
pub(crate) struct StartingTurnEnvironment {
    pub(crate) selection: TurnEnvironmentSelection,
    config_origin: EnvironmentConfigOrigin,
    resolution: TurnEnvironmentResolution,
}

/// Resolves config once at the attachment boundary and records who owns future updates.
fn resolve_selection_config(
    mut selection: TurnEnvironmentSelection,
    thread_config: &EnvironmentConfig,
) -> (TurnEnvironmentSelection, EnvironmentConfigOrigin) {
    let (config, origin) = match selection.config {
        EnvironmentConfigState::FromThread => (
            EnvironmentConfigState::Ready(thread_config_for_selection(
                &selection.workspace_roots,
                thread_config,
            )),
            EnvironmentConfigOrigin::Thread,
        ),
        config @ (EnvironmentConfigState::Ready(_)
        | EnvironmentConfigState::Pending
        | EnvironmentConfigState::Failed(_)) => (config, EnvironmentConfigOrigin::Owner),
    };
    selection.config = config;
    (selection, origin)
}

fn thread_config_for_selection(
    workspace_roots: &[PathUri],
    thread_config: &EnvironmentConfig,
) -> EnvironmentConfig {
    EnvironmentConfig {
        workspace_roots: workspace_roots.to_vec(),
        ..thread_config.clone()
    }
}

impl SelectedTurnEnvironment {
    fn refresh_thread_config(&mut self, config: &EnvironmentConfig) {
        if self.config_origin == EnvironmentConfigOrigin::Thread {
            self.selection.config = EnvironmentConfigState::Ready(thread_config_for_selection(
                &self.selection.workspace_roots,
                config,
            ));
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
    environments: ArcSwap<Vec<SelectedTurnEnvironment>>,
    connection_event_tx: OnceLock<Sender<Event>>,
}

impl ThreadEnvironments {
    pub(crate) fn new(
        environment_manager: Arc<EnvironmentManager>,
        local_shell: Shell,
        thread_environment_config: EnvironmentConfig,
        shell_snapshot: ShellSnapshot,
        current: TurnEnvironmentSnapshot,
        non_blocking_snapshots: bool,
    ) -> Self {
        // Reuse only attached environments from the supplied snapshot; drop starting entries.
        let environments = current
            .environments
            .into_iter()
            .filter_map(|environment| {
                let TurnEnvironmentState::Ready(environment) = environment else {
                    return None;
                };
                let selection = environment.selection;
                let config_origin = environment.config_origin;
                let selected_environment = Arc::clone(&environment.environment);
                let resolution: TurnEnvironmentResolution =
                    futures::future::ready(Ok(ResolvedEnvironment {
                        environment: environment.environment,
                        shell: environment.shell,
                        user_home_dir: environment.user_home_dir,
                        executor_platform_os: environment.executor_platform_os,
                        temporary_directories: environment.temporary_directories,
                        shell_snapshot: environment.shell_snapshot,
                        shell_snapshot_v2_supported: environment.shell_snapshot_v2_supported,
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
                };
                // Child threads re-infer thread-owned config while preserving owner config.
                inherited_environment.refresh_thread_config(&thread_environment_config);
                Some(inherited_environment)
            })
            .collect();
        Self {
            environment_manager,
            local_shell,
            shell_snapshot,
            non_blocking_snapshots,
            environments: ArcSwap::from_pointee(environments),
            connection_event_tx: OnceLock::new(),
        }
    }

    pub(crate) fn update_selections(
        &self,
        environments: &[TurnEnvironmentSelection],
        thread_environment_config: &EnvironmentConfig,
    ) {
        let previous = self.environments.load();
        let mut seen_environment_ids = HashSet::with_capacity(environments.len());
        let mut next = Vec::with_capacity(environments.len());
        for selected_environment in environments {
            if !seen_environment_ids.insert(selected_environment.environment_id.as_str()) {
                continue;
            }
            let (selected_environment, config_origin) =
                resolve_selection_config(selected_environment.clone(), thread_environment_config);
            if let Some(environment) = previous.iter().find(|environment| {
                let previous = &environment.selection;
                previous.environment_id == selected_environment.environment_id
                    && previous.cwd == selected_environment.cwd
                    && previous.workspace_roots == selected_environment.workspace_roots
            }) {
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
                    let mut environment = environment.clone();
                    environment.selection = selected_environment;
                    environment.config_origin = config_origin;
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
            let connection_events_task = previous
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
            let (pending_completion, configuration_ready) =
                if matches!(selected_environment.config, EnvironmentConfigState::Pending) {
                    let (sender, receiver) = mpsc::channel(/*buffer*/ 1);
                    (Some(sender), Some(receiver))
                } else {
                    (None, None)
                };
            let (resolution_task, resolution) = Self::resolve_environment(
                selected_environment.clone(),
                Arc::clone(&environment),
                self.local_shell.clone(),
                self.shell_snapshot.clone(),
                configuration_ready,
            )
            .remote_handle();
            drop(tokio::spawn(
                resolution_task.in_current_span().with_current_subscriber(),
            ));
            let resolution = resolution.boxed().shared();
            let selected = SelectedTurnEnvironment {
                selection: selected_environment,
                config_origin,
                environment,
                connection_events_task,
                resolution,
                pending_completion,
            };
            next.push(selected);
        }
        let removed_connection_tasks = previous
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
        let next = Arc::new(next);
        self.environments.store(Arc::clone(&next));

        // Publish owner configuration before waking turns waiting on this attachment.
        for environment in next.iter() {
            let Some(completion) = &environment.pending_completion else {
                continue;
            };
            let result = match &environment.selection.config {
                EnvironmentConfigState::Ready(config) => Ok(config.clone()),
                EnvironmentConfigState::Failed(error) => Err(error.clone()),
                EnvironmentConfigState::FromThread | EnvironmentConfigState::Pending => continue,
            };
            let _ = completion.try_send(result);
        }

        // ArcSwap readers may retain removed selections, so abort at logical removal.
        for task in removed_connection_tasks {
            task.abort();
        }
    }

    /// Projects canonical selections back to caller input form without losing config ownership.
    pub(crate) fn selections(&self) -> Vec<TurnEnvironmentSelection> {
        self.environments
            .load()
            .iter()
            .map(|environment| {
                environment
                    .config_origin
                    .into_input_selection(environment.selection.clone())
            })
            .collect()
    }

    pub(crate) fn primary_workspace_roots(&self) -> Vec<AbsolutePathBuf> {
        self.environments
            .load()
            .first()
            .map_or_else(Vec::new, |environment| {
                Self::primary_workspace_roots_for(std::slice::from_ref(&environment.selection))
            })
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

    /// Refreshes attachments whose configuration is inferred from the thread.
    pub(crate) fn update_thread_config(&self, config: &EnvironmentConfig) {
        let environments = self
            .environments
            .load()
            .iter()
            .map(|environment| {
                let mut environment = environment.clone();
                environment.refresh_thread_config(config);
                environment
            })
            .collect();
        self.environments.store(Arc::new(environments));
    }

    /// Combines persisted thread roots with installed attachment roots, keeping
    /// thread roots first and hiding attachments that are not ready yet.
    pub(crate) fn inspect_selected_capability_roots(
        &self,
        thread_selected_capability_roots: &[SelectedCapabilityRoot],
    ) -> SelectedCapabilityRootsStatus {
        let environments = self.environments.load();
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
        let tx_event = self.connection_event_tx.get_or_init(|| tx_event);
        let current = self.environments.load_full();
        let environments = current
            .iter()
            .map(|selected| {
                let mut selected = selected.clone();
                if selected.connection_events_task.is_none() {
                    selected.connection_events_task = Self::spawn_connection_event_listener(
                        selected.environment.as_ref(),
                        selected.selection.environment_id.clone(),
                        tx_event.clone(),
                    );
                }
                selected
            })
            .collect();
        self.environments.store(Arc::new(environments));
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
        configuration_ready: Option<mpsc::Receiver<PendingConfigurationResult>>,
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
        // Wait for this thread attachment's owner configuration, if pending.
        let configuration_ready = async move {
            let Some(mut configuration_ready) = configuration_ready else {
                return Ok(None);
            };
            match configuration_ready.recv().await {
                Some(Ok(config)) => Ok(Some(config)),
                Some(Err(error)) => Err(Arc::new(ExecServerError::Protocol(error))),
                None => Err(Arc::new(ExecServerError::Protocol(
                    "environment configuration was canceled".to_string(),
                ))),
            }
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
        let task = shell_snapshot
            .build(Arc::clone(&environment), selection.cwd, shell.clone())
            .boxed()
            .shared();
        drop(tokio::spawn(
            task.clone().in_current_span().with_current_subscriber(),
        ));
        Ok(ResolvedEnvironment {
            environment,
            shell,
            user_home_dir,
            executor_platform_os,
            temporary_directories: temporary_dirs,
            shell_snapshot: task,
            shell_snapshot_v2_supported: snapshot_v2,
            installed_config,
        })
    }

    #[tracing::instrument(
        name = "environments.snapshot",
        skip_all,
        fields(
            environment_count = self.environments.load().len(),
            non_blocking = self.non_blocking_snapshots,
        )
    )]
    pub(crate) async fn snapshot(&self) -> TurnEnvironmentSnapshot {
        let selected = self.environments.load_full();
        let mut environments = Vec::with_capacity(selected.len());
        for environment in selected.iter() {
            if matches!(
                environment.selection.config,
                EnvironmentConfigState::Failed(_)
            ) {
                environments.push(TurnEnvironmentState::Failed);
                continue;
            }
            let pending = matches!(
                environment.selection.config,
                EnvironmentConfigState::Pending
            );
            let starting = StartingTurnEnvironment {
                selection: environment.selection.clone(),
                config_origin: environment.config_origin,
                resolution: environment.resolution.clone(),
            };
            let resolved = if self.non_blocking_snapshots || pending {
                starting.resolution.clone().now_or_never()
            } else {
                Some(match starting.wait_until_ready().await {
                    Ok(()) => starting.resolution.clone().await,
                    Err(error) => Err(error),
                })
            };
            environments.push(TurnEnvironmentState::from_resolution(starting, resolved));
        }
        TurnEnvironmentSnapshot { environments }
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
    Failed,
}

impl TurnEnvironmentState {
    fn from_resolution(
        starting: StartingTurnEnvironment,
        resolved: Option<TurnEnvironmentResult>,
    ) -> Self {
        match resolved {
            Some(Ok(environment)) => {
                let mut selection = starting.selection;
                if matches!(selection.config, EnvironmentConfigState::Pending) {
                    let Some(config) = environment.installed_config else {
                        return Self::Failed;
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
                Self::Failed
            }
            None => Self::Starting(starting),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct TurnEnvironmentSnapshot {
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
                    TurnEnvironmentState::Starting(_) | TurnEnvironmentState::Failed => {
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
                TurnEnvironmentState::Failed => TurnEnvironmentState::Failed,
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
                TurnEnvironmentState::Starting(_) | TurnEnvironmentState::Failed => None,
            })
    }


    pub(crate) fn to_selections(&self) -> Vec<TurnEnvironmentSelection> {
        self.turn_environments()
            .map(TurnEnvironment::selection)
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
