use super::*;

#[derive(Clone)]
pub(crate) struct WindowsSandboxRequestProcessor {
    outgoing: Arc<OutgoingMessageSender>,
    config: Arc<Config>,
    config_manager: ConfigManager,
}

impl WindowsSandboxRequestProcessor {
    pub(crate) fn new(
        outgoing: Arc<OutgoingMessageSender>,
        config: Arc<Config>,
        config_manager: ConfigManager,
    ) -> Self {
        Self {
            outgoing,
            config,
            config_manager,
        }
    }

    pub(crate) async fn windows_sandbox_readiness(
        &self,
    ) -> Result<WindowsSandboxReadinessResponse, JSONRPCErrorError> {
        Ok(determine_windows_sandbox_readiness(&self.config))
    }

    pub(crate) async fn windows_sandbox_setup_start(
        &self,
        request_id: &ConnectionRequestId,
        params: WindowsSandboxSetupStartParams,
    ) -> Result<Option<ClientResponsePayload>, JSONRPCErrorError> {
        self.windows_sandbox_setup_start_inner(request_id, params)
            .await
            .map(|()| None)
    }

    async fn windows_sandbox_setup_start_inner(
        &self,
        request_id: &ConnectionRequestId,
        params: WindowsSandboxSetupStartParams,
    ) -> Result<(), JSONRPCErrorError> {
        // Validate requirements before acknowledging setup so callers do not get a
        // `started` response for a Windows sandbox mode that cannot be persisted.
        let command_cwd = params
            .cwd
            .map(PathBuf::from)
            .unwrap_or_else(|| self.config.cwd.to_path_buf());
        let config = self
            .config_manager
            .load_for_cwd(
                /*request_overrides*/ None,
                ConfigOverrides {
                    cwd: Some(command_cwd.clone()),
                    ..Default::default()
                },
                Some(command_cwd.clone()),
            )
            .await
            .map_err(|err| config_load_error(&err))?;
        let setup_mode = resolve_allowed_windows_sandbox_setup_mode(
            config.config_layer_stack.requirements(),
            params.mode,
        )?;

        self.outgoing
            .send_response(
                request_id.clone(),
                WindowsSandboxSetupStartResponse { started: true },
            )
            .await;

        let outgoing = Arc::clone(&self.outgoing);
        let connection_id = request_id.connection_id;

        tokio::spawn(async move {
            let setup_request = WindowsSandboxSetupRequest {
                mode: setup_mode,
                permission_profile: config.permissions.effective_permission_profile(),
                workspace_roots: config.effective_workspace_roots(),
                command_cwd,
                env_map: std::env::vars().collect(),
                codex_home: config.codex_home.to_path_buf(),
            };
            let setup_result =
                codex_core::windows_sandbox::run_windows_sandbox_setup(setup_request).await;
            let notification = WindowsSandboxSetupCompletedNotification {
                mode: match setup_mode {
                    CoreWindowsSandboxSetupMode::Elevated => WindowsSandboxSetupMode::Elevated,
                    CoreWindowsSandboxSetupMode::Unelevated => WindowsSandboxSetupMode::Unelevated,
                },
                success: setup_result.is_ok(),
                error: setup_result.err().map(|err| err.to_string()),
            };
            outgoing
                .send_server_notification_to_connections(
                    &[connection_id],
                    ServerNotification::WindowsSandboxSetupCompleted(notification),
                )
                .await;
        });
        Ok(())
    }
}

/// Resolves the requested API mode after checking that managed requirements allow it.
fn resolve_allowed_windows_sandbox_setup_mode(
    requirements: &codex_config::ConfigRequirements,
    requested_mode: WindowsSandboxSetupMode,
) -> Result<CoreWindowsSandboxSetupMode, JSONRPCErrorError> {
    let (setup_mode, config_mode) = match requested_mode {
        WindowsSandboxSetupMode::Elevated => (
            CoreWindowsSandboxSetupMode::Elevated,
            codex_config::types::WindowsSandboxModeToml::Elevated,
        ),
        WindowsSandboxSetupMode::Unelevated => (
            CoreWindowsSandboxSetupMode::Unelevated,
            codex_config::types::WindowsSandboxModeToml::Unelevated,
        ),
    };
    requirements
        .windows_sandbox_mode
        .can_set(&Some(config_mode))
        .map_err(|err| invalid_request(format!("invalid Windows sandbox setup mode: {err}")))?;
    Ok(setup_mode)
}

fn determine_windows_sandbox_readiness(config: &Config) -> WindowsSandboxReadinessResponse {
    if !cfg!(windows) {
        return WindowsSandboxReadinessResponse {
            status: WindowsSandboxReadiness::NotConfigured,
        };
    }

    determine_windows_sandbox_readiness_from_state(
        WindowsSandboxLevel::from_config(config),
        sandbox_setup_is_complete(config.codex_home.as_path()),
    )
}

fn determine_windows_sandbox_readiness_from_state(
    windows_sandbox_level: WindowsSandboxLevel,
    sandbox_setup_is_complete: bool,
) -> WindowsSandboxReadinessResponse {
    let status = match windows_sandbox_level {
        WindowsSandboxLevel::Disabled => WindowsSandboxReadiness::NotConfigured,
        WindowsSandboxLevel::RestrictedToken => WindowsSandboxReadiness::Ready,
        WindowsSandboxLevel::Elevated => {
            if sandbox_setup_is_complete {
                WindowsSandboxReadiness::Ready
            } else {
                WindowsSandboxReadiness::UpdateRequired
            }
        }
    };

    WindowsSandboxReadinessResponse { status }
}
