/*
Runtime: unified exec

Handles approval + sandbox orchestration for unified exec requests, delegating to
the process manager to spawn PTYs once an ExecRequest is prepared.
*/
use crate::exec::ExecCapturePolicy;
use crate::exec::ExecExpiration;
use crate::guardian::GUARDIAN_REVIEW_TIMEOUT;
use crate::guardian::GuardianNetworkAccessTrigger;
use crate::guardian::routes_approval_policy_to_guardian;
use crate::plugins::metrics::sidecar_for_command;
use crate::sandboxing::ExecOptions;
use crate::sandboxing::ExecServerEnvConfig;
use crate::sandboxing::SandboxPermissions;
use crate::session::turn_context::TurnEnvironment;
use crate::shell::ShellType;
use crate::tools::flat_tool_name;
use crate::tools::network_approval::NetworkApprovalSpec;
use crate::tools::runtimes::RuntimePathPrepends;
#[cfg(unix)]
use crate::tools::runtimes::apply_zsh_fork_path_prepend;
use crate::tools::runtimes::exec_env_for_sandbox_permissions;
use crate::tools::runtimes::maybe_wrap_shell_lc_with_snapshot;
use crate::tools::runtimes::prepare_powershell_command_for_elevated_windows_sandbox;
use crate::tools::runtimes::zsh_fork;
use crate::tools::sandboxing::Approvable;
use crate::tools::sandboxing::ApprovalAction;
use crate::tools::sandboxing::ExecApprovalRequirement;
use crate::tools::sandboxing::SandboxAttempt;
use crate::tools::sandboxing::Sandboxable;
use crate::tools::sandboxing::ToolCtx;
use crate::tools::sandboxing::ToolError;
use crate::tools::sandboxing::ToolRuntime;
use crate::tools::sandboxing::managed_network_for_sandbox_permissions;
use crate::tools::sandboxing::sandbox_permissions_preserving_denied_reads;
use crate::unified_exec::NoopSpawnLifecycle;
use crate::unified_exec::TerminalPermissions;
use crate::unified_exec::TerminalSandboxSource;
use crate::unified_exec::UnifiedExecError;
use crate::unified_exec::UnifiedExecProcess;
use crate::unified_exec::UnifiedExecProcessManager;
use codex_core_plugins::PluginMetricsSidecar;
use codex_network_proxy::ManagedNetworkSandboxContext;
use codex_network_proxy::NetworkProxy;
use codex_protocol::error::CodexErr;
use codex_protocol::error::SandboxErr;
use codex_protocol::models::AdditionalPermissionProfile;
use codex_sandboxing::SandboxCommand;
use codex_sandboxing::SandboxablePreference;
use codex_sandboxing::policy_transforms::merge_permission_profiles;
use codex_shell_command::powershell::prefix_powershell_script_with_utf8;
use codex_tools::UnifiedExecShellMode;
use codex_utils_path_uri::PathUri;
use std::collections::HashMap;
use std::io;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

// Allow 5s for Guardian cleanup and 5s for controller processing after review.
const REMOTE_NETWORK_POLICY_DECISION_MARGIN: Duration = Duration::from_secs(10);

/// Request payload used by the unified-exec runtime after approvals and
/// sandbox preferences have been resolved for the current turn.
#[derive(Clone, Debug)]
pub struct UnifiedExecRequest {
    pub command: Vec<String>,
    pub shell_type: ShellType,
    pub hook_command: String,
    pub process_id: i32,
    pub cwd: PathUri,
    pub sandbox_cwd: PathUri,
    pub turn_environment: TurnEnvironment,
    pub env: HashMap<String, String>,
    pub exec_server_env_config: Option<ExecServerEnvConfig>,
    pub shell_snapshot: Option<codex_exec_server::ShellSnapshotRequest>,
    pub explicit_env_overrides: HashMap<String, String>,
    pub network: Option<NetworkProxy>,
    pub tty: bool,
    pub sandbox_permissions: SandboxPermissions,
    pub additional_permissions: Option<AdditionalPermissionProfile>,
    #[cfg(unix)]
    pub additional_permissions_preapproved: bool,
    pub justification: Option<String>,
    pub exec_approval_requirement: ExecApprovalRequirement,
}

/// Cache key for approval decisions that can be reused across equivalent
/// unified-exec launches.
#[derive(serde::Serialize, Clone, Debug, Eq, PartialEq, Hash)]
pub struct UnifiedExecApprovalKey {
    pub environment_id: String,
    pub executable: Option<String>,
    pub command: Vec<String>,
    pub cwd: PathUri,
    pub tty: bool,
    pub sandbox_permissions: SandboxPermissions,
    pub additional_permissions: Option<AdditionalPermissionProfile>,
}

/// Runtime adapter that keeps policy and sandbox orchestration on the
/// unified-exec side while delegating process startup to the manager.
pub struct UnifiedExecRuntime<'a> {
    manager: &'a UnifiedExecProcessManager,
    shell_mode: UnifiedExecShellMode,
}

pub(crate) struct UnifiedExecAttempt {
    pub(crate) process: UnifiedExecProcess,
    pub(crate) metrics_sidecar: Option<PluginMetricsSidecar>,
    pub(crate) permissions: TerminalPermissions,
}

fn unified_exec_options(
    network_denial_cancellation_token: Option<CancellationToken>,
) -> ExecOptions {
    let mut expiration = ExecExpiration::DefaultTimeout;
    if let Some(cancellation) = network_denial_cancellation_token {
        expiration = expiration.with_cancellation(cancellation);
    }
    ExecOptions {
        expiration,
        capture_policy: ExecCapturePolicy::ShellTool,
    }
}

fn build_unified_exec_sandbox_command(
    command: &[String],
    cwd: &PathUri,
    env: &HashMap<String, String>,
    managed_network: Option<ManagedNetworkSandboxContext>,
    additional_permissions: Option<AdditionalPermissionProfile>,
) -> Result<SandboxCommand, ToolError> {
    let (program, args) = command
        .split_first()
        .ok_or_else(|| ToolError::Rejected("command args are empty".to_string()))?;
    Ok(SandboxCommand {
        program: program.clone().into(),
        args: args.to_vec(),
        cwd: cwd.clone(),
        env: env.clone(),
        managed_network,
        additional_permissions,
    })
}

impl<'a> UnifiedExecRuntime<'a> {
    /// Creates a runtime bound to the shared unified-exec process manager.
    pub fn new(manager: &'a UnifiedExecProcessManager, shell_mode: UnifiedExecShellMode) -> Self {
        Self {
            manager,
            shell_mode,
        }
    }
}

impl Sandboxable for UnifiedExecRuntime<'_> {
    fn sandbox_preference(&self) -> SandboxablePreference {
        SandboxablePreference::Auto
    }

    fn escalate_on_failure(&self) -> bool {
        true
    }
}

impl Approvable<UnifiedExecRequest> for UnifiedExecRuntime<'_> {
    fn approval_action(
        &self,
        req: &UnifiedExecRequest,
        call_id: &str,
    ) -> std::io::Result<ApprovalAction> {
        Ok(ApprovalAction::ExecCommand {
            id: call_id.to_string(),
            environment_id: req.turn_environment.selection.environment_id.clone(),
            command: req.command.clone(),
            hook_command: req.hook_command.clone(),
            cwd: req.cwd.clone(),
            sandbox_permissions: req.sandbox_permissions,
            additional_permissions: req.additional_permissions.clone(),
            justification: req.justification.clone(),
            tty: req.tty,
            proposed_execpolicy_amendment: req
                .exec_approval_requirement
                .proposed_execpolicy_amendment()
                .cloned(),
        })
    }

    fn exec_approval_requirement(
        &self,
        req: &UnifiedExecRequest,
    ) -> Option<ExecApprovalRequirement> {
        Some(req.exec_approval_requirement.clone())
    }

    fn sandbox_permissions(&self, req: &UnifiedExecRequest) -> SandboxPermissions {
        req.sandbox_permissions
    }
}

impl<'a> ToolRuntime<UnifiedExecRequest, UnifiedExecAttempt> for UnifiedExecRuntime<'a> {
    fn turn_environment<'b>(&self, req: &'b UnifiedExecRequest) -> &'b TurnEnvironment {
        &req.turn_environment
    }

    fn uses_executor_managed_process_sandbox(&self, req: &UnifiedExecRequest) -> bool {
        req.turn_environment.environment.is_remote() || req.shell_snapshot.is_some()
    }

    fn sandbox_cwd<'b>(&self, req: &'b UnifiedExecRequest) -> Option<&'b PathUri> {
        Some(&req.sandbox_cwd)
    }

    fn network_approval_spec(
        &self,
        req: &UnifiedExecRequest,
        ctx: &ToolCtx,
    ) -> Option<NetworkApprovalSpec> {
        let file_system_sandbox_policy = req
            .turn_environment
            .permission_profile()
            .file_system_sandbox_policy();
        let sandbox_permissions = sandbox_permissions_preserving_denied_reads(
            req.sandbox_permissions,
            &file_system_sandbox_policy,
        );
        let network =
            managed_network_for_sandbox_permissions(req.network.as_ref(), sandbox_permissions)
                .cloned();
        // No-proxy fast path; owners still need a spec for execution-only proxies.
        if network.is_none() && req.turn_environment.config().network_policy.is_none() {
            return None;
        }
        Some(NetworkApprovalSpec {
            network,
            tool_name: ctx.tool_name.clone(),
            trigger: GuardianNetworkAccessTrigger {
                call_id: ctx.call_id.clone(),
                tool_name: flat_tool_name(&ctx.tool_name).into_owned(),
                command: req.command.clone(),
                cwd: req.cwd.clone(),
                sandbox_permissions: req.sandbox_permissions,
                additional_permissions: req.additional_permissions.clone(),
                justification: req.justification.clone(),
                tty: Some(req.tty),
            },
            command: req.hook_command.clone(),
            environment_id: req.turn_environment.selection.environment_id.clone(),
            permission_profile: req.turn_environment.permission_profile().clone(),
            network_policy: req.turn_environment.config().network_policy.clone(),
        })
    }

    async fn run(
        &mut self,
        req: &UnifiedExecRequest,
        attempt: &SandboxAttempt<'_>,
        ctx: &ToolCtx,
    ) -> Result<UnifiedExecAttempt, ToolError> {
        let base_command = &req.command;
        let windows_sandbox_proxy_settings_mode = ctx.session.windows_sandbox_proxy_settings_mode;
        let session_shell = ctx.session.user_shell();
        let shell = req
            .turn_environment
            .shell
            .as_ref()
            .unwrap_or(session_shell.as_ref());
        let environment_is_remote = req.turn_environment.environment.is_remote();
        let shell_snapshot_location = if environment_is_remote {
            None
        } else {
            // TODO(anp): Make shell snapshot lookup accept PathUri.
            let native_cwd = req
                .cwd
                .to_abs_path()
                .map_err(|err| ToolError::Rejected(err.to_string()))?;
            req.turn_environment.shell_snapshot(&native_cwd)
        };
        let (file_system_sandbox_policy, _) = attempt.permissions.to_runtime_permissions();
        let launch_sandbox_permissions = sandbox_permissions_preserving_denied_reads(
            req.sandbox_permissions,
            &file_system_sandbox_policy,
        );
        let managed_network = attempt.network_proxy(managed_network_for_sandbox_permissions(
            req.network.as_ref(),
            launch_sandbox_permissions,
        ));
        let env = exec_env_for_sandbox_permissions(&req.env, launch_sandbox_permissions);
        let (mut env, managed_network_context, network_proxy_launch) = match managed_network {
            Some(network) if environment_is_remote => {
                let mut launch = network.remote_launch_config().await.map_err(|err| {
                    ToolError::Codex(CodexErr::Io(io::Error::other(err.to_string())))
                })?;
                if routes_approval_policy_to_guardian(
                    ctx.step_context.settings.approval_policy(),
                    ctx.step_context.settings.approvals_reviewer(),
                ) && network.remote_policy_decider().is_some()
                {
                    let timeout = ctx
                        .session
                        .hooks()
                        .max_permission_request_timeout()
                        .saturating_add(GUARDIAN_REVIEW_TIMEOUT)
                        .saturating_add(REMOTE_NETWORK_POLICY_DECISION_MARGIN);
                    launch.policy_decision_timeout_ms =
                        Some(u64::try_from(timeout.as_millis()).map_err(|_| {
                            ToolError::Rejected(
                                "remote network policy decision timeout exceeds protocol limit"
                                    .to_string(),
                            )
                        })?);
                }
                if !launch.proxy.enabled {
                    (env, None, None)
                } else {
                    let environment_info =
                        req.turn_environment
                            .environment
                            .info()
                            .await
                            .map_err(|err| {
                                ToolError::Codex(CodexErr::Io(io::Error::other(format!(
                                    "failed to query exec-server capabilities: {err}"
                                ))))
                            })?;
                    if !environment_info.capabilities.network_proxy_launch {
                        return Err(ToolError::Rejected(
                            "selected exec-server does not support executor-local network proxy launches"
                                .to_string(),
                        ));
                    }
                    (env, None, Some(launch))
                }
            }
            Some(network) => {
                let prepared = network
                    .prepare_for_optional_environment(
                        env,
                        Some(&req.turn_environment.selection.environment_id),
                    )
                    .map_err(|err| {
                        ToolError::Codex(CodexErr::Io(io::Error::other(format!(
                            "failed to prepare network proxy for environment `{}`: {err}",
                            req.turn_environment.selection.environment_id
                        ))))
                    })?;
                (prepared.env, Some(prepared.sandbox_context), None)
            }
            None => (env, None, None),
        };
        let explicit_env_overrides = req.explicit_env_overrides.clone();
        let metrics_sidecar = sidecar_for_command(
            ctx,
            &req.command,
            &req.cwd,
            req.turn_environment.environment.as_ref(),
        )
        .await;
        if let Some(sidecar) = metrics_sidecar.as_ref() {
            sidecar.install_output_env(&mut env);
        }
        #[cfg(unix)]
        let runtime_path_prepends = {
            let mut runtime_path_prepends = RuntimePathPrepends::default();
            if !environment_is_remote {
                crate::tools::runtimes::apply_package_path_prepend(
                    &mut env,
                    &mut runtime_path_prepends,
                );
            }
            if let UnifiedExecShellMode::ZshFork(zsh_fork_config) = &self.shell_mode {
                apply_zsh_fork_path_prepend(
                    &mut env,
                    &mut runtime_path_prepends,
                    zsh_fork_config.shell_zsh_path.as_path(),
                );
            }
            runtime_path_prepends
        };
        #[cfg(not(unix))]
        let runtime_path_prepends = RuntimePathPrepends::default();
        let mut command = if environment_is_remote {
            base_command.to_vec()
        } else {
            maybe_wrap_shell_lc_with_snapshot(
                base_command,
                shell,
                shell_snapshot_location.as_ref(),
                &explicit_env_overrides,
                &env,
                &runtime_path_prepends,
            )
        };
        if req.shell_snapshot.is_some() {
            let exports =
                runtime_path_prepends.shell_exports_after_snapshot(&explicit_env_overrides);
            if !exports.is_empty()
                && let Some(script) = command.get_mut(2)
            {
                *script = format!("{exports}\n{script}");
            }
        }
        let command = prepare_powershell_command_for_elevated_windows_sandbox(
            &command,
            Some(&req.shell_type),
            attempt.sandbox_requested,
            attempt.windows_sandbox_level,
            environment_is_remote,
        );
        let command = if matches!(req.shell_type, ShellType::PowerShell) {
            prefix_powershell_script_with_utf8(&command)
        } else {
            command
        };
        let sidecar_permissions = metrics_sidecar
            .as_ref()
            .map(PluginMetricsSidecar::additional_permissions);
        let additional_permissions = merge_permission_profiles(
            req.additional_permissions.as_ref(),
            sidecar_permissions.as_ref(),
        );
        let permissions = TerminalPermissions::for_launch(
            &req.turn_environment,
            &ctx.step_context.turn,
            if self.uses_executor_managed_process_sandbox(req) {
                TerminalSandboxSource::Executor
            } else {
                TerminalSandboxSource::Native
            },
            if attempt.is_escalated() {
                SandboxPermissions::RequireEscalated
            } else {
                SandboxPermissions::UseDefault
            },
            req.additional_permissions.as_ref(),
            sidecar_permissions.as_ref(),
        );

        if let UnifiedExecShellMode::ZshFork(zsh_fork_config) = &self.shell_mode {
            let command = build_unified_exec_sandbox_command(
                &command,
                &req.cwd,
                &env,
                managed_network_context.clone(),
                additional_permissions.clone(),
            )
            .map_err(|error| match error {
                ToolError::Rejected(_) => {
                    ToolError::Rejected("missing command line for PTY".to_string())
                }
                error @ ToolError::Codex(_) => error,
            })?;
            let options = unified_exec_options(attempt.network_denial_cancellation_token.clone());
            let mut exec_env = attempt
                .env_for(
                    command,
                    options,
                    managed_network,
                    Some(&req.turn_environment.selection.environment_id),
                )
                .map_err(ToolError::Codex)?;
            exec_env.exec_server_env_config = req.exec_server_env_config.clone();
            match zsh_fork::maybe_prepare_unified_exec(req, attempt, ctx, exec_env, zsh_fork_config)
                .await?
            {
                Some(prepared) => {
                    if req.turn_environment.environment.is_remote() {
                        return Err(ToolError::Rejected(
                            "unified_exec zsh-fork is not supported for remote environments"
                                .to_string(),
                        ));
                    }
                    let process = self
                        .manager
                        .open_session_with_prepared_exec_env(
                            req.process_id,
                            &prepared.exec_request,
                            windows_sandbox_proxy_settings_mode,
                            /*network_policy_decider*/ None,
                            req.tty,
                            prepared.spawn_lifecycle,
                            req.turn_environment.environment.as_ref(),
                        )
                        .await
                        .map_err(|err| match err {
                            UnifiedExecError::SandboxDenied { output, .. } => {
                                ToolError::Codex(CodexErr::Sandbox(SandboxErr::Denied {
                                    output: Box::new(output),
                                    network_policy_decision: None,
                                }))
                            }
                            other => ToolError::Rejected(other.to_string()),
                        })?;
                    return Ok(UnifiedExecAttempt {
                        process,
                        metrics_sidecar,
                        permissions,
                    });
                }
                None => {
                    tracing::warn!(
                        "UnifiedExec ZshFork backend specified, but conditions for using it were not met, falling back to direct execution",
                    );
                }
            }
        }
        let command = build_unified_exec_sandbox_command(
            &command,
            &req.cwd,
            &env,
            managed_network_context,
            additional_permissions,
        )
        .map_err(|error| match error {
            ToolError::Rejected(_) => {
                ToolError::Rejected("missing command line for PTY".to_string())
            }
            error @ ToolError::Codex(_) => error,
        })?;
        let options = unified_exec_options(attempt.network_denial_cancellation_token.clone());
        let process = self
            .manager
            .open_session_with_exec_env(
                req.process_id,
                command,
                options,
                attempt,
                managed_network,
                network_proxy_launch,
                /*environment_id*/ Some(&req.turn_environment.selection.environment_id),
                req.exec_server_env_config.clone(),
                req.shell_snapshot.clone(),
                windows_sandbox_proxy_settings_mode,
                req.tty,
                Box::new(NoopSpawnLifecycle),
                req.turn_environment.environment.as_ref(),
            )
            .await?;
        Ok(UnifiedExecAttempt {
            process,
            metrics_sidecar,
            permissions,
        })
    }
}
