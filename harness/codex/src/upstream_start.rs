//! Production construction of Codex's in-process App Server client.

use std::path::PathBuf;
use std::sync::Arc;

use codex_app_server_client::{InProcessAppServerClient, InProcessClientStartArgs};
use codex_app_server_protocol::ConfigWarningNotification;
use codex_arg0::Arg0DispatchPaths;
use codex_config::{CloudConfigBundleLoader, LoaderOverrides};
use codex_core::config::{ConfigBuilder, ConfigOverrides};
use codex_exec_server::{EnvironmentManager, ExecServerRuntimePaths};
use codex_feedback::CodexFeedback;
use codex_protocol::protocol::SessionSource;
use codex_utils_absolute_path::AbsolutePathBuf;

use crate::diagnostics::StartupStage;
use crate::{CapabilitySet, HarnessConfig, UpstreamError};

#[derive(Debug, Clone)]
pub struct UpstreamStartArgs {
    pub harness: HarnessConfig,
    pub runtime_fingerprint: String,
    pub capabilities: CapabilitySet,
    pub codex_home: PathBuf,
    pub cwd: PathBuf,
    pub workspace_roots: Vec<PathBuf>,
    pub helper_executable: PathBuf,
    pub linux_sandbox_executable: Option<PathBuf>,
    pub main_execve_wrapper_executable: Option<PathBuf>,
    pub enable_codex_api_key_env: bool,
    pub mcp_server_openai_form_elicitation: bool,
    pub opt_out_notification_methods: Vec<String>,
}

impl UpstreamStartArgs {
    pub(crate) async fn build_client(self) -> Result<InProcessAppServerClient, UpstreamError> {
        let stage = StartupStage::new("validate_paths");
        let workspace_roots = stage.finish(validate_paths(&self)).map_err(start_error)?;
        let arg0_paths = Arg0DispatchPaths {
            codex_self_exe: Some(self.helper_executable.clone()),
            codex_linux_sandbox_exe: self.linux_sandbox_executable.clone(),
            main_execve_wrapper_exe: self.main_execve_wrapper_executable.clone(),
        };
        let launch_overrides = crate::launch_config::environment_overrides()?;
        let stage = StartupStage::new("load_config");
        let config = stage
            .finish(build_config(&self, &arg0_paths, (workspace_roots, &launch_overrides)).await)
            .map_err(start_error)?;
        let args = self
            .client_start_args(config, (arg0_paths, launch_overrides))
            .await?;
        StartupStage::new("app_server")
            .finish(InProcessAppServerClient::start(args).await)
            .map_err(start_error)
    }

    async fn client_start_args(
        &self,
        config: codex_core::config::Config,
        launch: (Arg0DispatchPaths, serde_json::Value),
    ) -> Result<InProcessClientStartArgs, UpstreamError> {
        let (arg0_paths, launch_overrides) = launch;
        let environment_manager = build_environment(&config, &arg0_paths).await?;
        let stage = StartupStage::new("state_db");
        let state_db = codex_core::init_state_db(&config).await;
        stage.complete();
        eprintln!(
            "[internal-codex-worker] stage=state_db available={}",
            state_db.is_some()
        );
        let config_warnings = config
            .startup_warnings
            .iter()
            .map(|summary| ConfigWarningNotification {
                summary: summary.clone(),
                details: None,
                path: None,
                range: None,
            })
            .collect();
        Ok(InProcessClientStartArgs {
            arg0_paths,
            config: Arc::new(config),
            cli_overrides: serde_json::from_value(launch_overrides).map_err(|_| {
                crate::launch_config::invalid("CODEX_CONFIG has unsupported TOML values")
            })?,
            loader_overrides: LoaderOverrides::default(),
            strict_config: true,
            cloud_config_bundle: CloudConfigBundleLoader::default(),
            feedback: CodexFeedback::new(),
            log_db: None,
            state_db,
            environment_manager: Arc::new(environment_manager),
            config_warnings,
            session_source: SessionSource::Custom("iyw-claw".to_string()),
            enable_codex_api_key_env: self.enable_codex_api_key_env,
            client_name: self.harness.client_name.clone(),
            client_version: self.harness.client_version.clone(),
            experimental_api: self.harness.experimental_api,
            mcp_server_openai_form_elicitation: self.mcp_server_openai_form_elicitation,
            opt_out_notification_methods: self.opt_out_notification_methods.clone(),
            channel_capacity: self.harness.channel_capacity,
        })
    }
}

async fn build_environment(
    config: &codex_core::config::Config,
    arg0_paths: &Arg0DispatchPaths,
) -> Result<EnvironmentManager, UpstreamError> {
    StartupStage::new("environment_manager")
        .finish(
            async {
                let runtime_paths = ExecServerRuntimePaths::from_optional_paths(
                    arg0_paths.codex_self_exe.clone(),
                    arg0_paths.codex_linux_sandbox_exe.clone(),
                )
                .map_err(start_error)?;
                EnvironmentManager::from_codex_home(
                    config.codex_home.clone(),
                    Some(runtime_paths),
                    config.http_client_factory(),
                )
                .await
                .map_err(start_error)
            }
            .await,
        )
        .map_err(start_error)
}

async fn build_config(
    args: &UpstreamStartArgs,
    arg0_paths: &Arg0DispatchPaths,
    launch: (Vec<AbsolutePathBuf>, &serde_json::Value),
) -> Result<codex_core::config::Config, UpstreamError> {
    let (workspace_roots, overrides_json) = launch;
    let overrides = ConfigOverrides {
        cwd: Some(canonical_cwd(args)?),
        codex_self_exe: arg0_paths.codex_self_exe.clone(),
        codex_linux_sandbox_exe: arg0_paths.codex_linux_sandbox_exe.clone(),
        main_execve_wrapper_exe: arg0_paths.main_execve_wrapper_exe.clone(),
        workspace_roots: Some(workspace_roots),
        ..Default::default()
    };
    ConfigBuilder::default()
        .cli_overrides(serde_json::from_value(overrides_json.clone()).map_err(|_| {
            crate::launch_config::invalid("CODEX_CONFIG has unsupported TOML values")
        })?)
        .codex_home(args.codex_home.clone())
        .fallback_cwd(Some(args.cwd.clone()))
        .harness_overrides(overrides)
        .loader_overrides(LoaderOverrides::default())
        .strict_config(true)
        .cloud_config_bundle(CloudConfigBundleLoader::default())
        .build()
        .await
        .map_err(start_error)
}

fn validate_paths(args: &UpstreamStartArgs) -> Result<Vec<AbsolutePathBuf>, UpstreamError> {
    for (name, path) in [
        ("Codex home", &args.codex_home),
        ("working directory", &args.cwd),
        ("helper executable", &args.helper_executable),
    ] {
        if !path.is_absolute() {
            return Err(UpstreamError::Start(format!(
                "{name} must be an absolute path"
            )));
        }
    }
    if !args.helper_executable.is_file() {
        return Err(UpstreamError::Start(
            "helper executable does not exist".to_string(),
        ));
    }
    validate_optional_helper(
        "Linux sandbox helper",
        args.linux_sandbox_executable.as_deref(),
    )?;
    validate_optional_helper(
        "execve wrapper helper",
        args.main_execve_wrapper_executable.as_deref(),
    )?;
    workspace_roots(args)
}

fn validate_optional_helper(
    name: &str,
    path: Option<&std::path::Path>,
) -> Result<(), UpstreamError> {
    let Some(path) = path else {
        return Ok(());
    };
    if path.is_absolute() && path.is_file() {
        return Ok(());
    }
    Err(UpstreamError::Start(format!(
        "{name} must be an existing absolute file"
    )))
}

fn workspace_roots(args: &UpstreamStartArgs) -> Result<Vec<AbsolutePathBuf>, UpstreamError> {
    let cwd = canonical_cwd(args)?;
    if args.workspace_roots.is_empty() {
        return Err(UpstreamError::Start(
            "at least one workspace root is required".to_string(),
        ));
    }
    let roots = args
        .workspace_roots
        .iter()
        .map(std::fs::canonicalize)
        .collect::<Result<Vec<_>, _>>()
        .map_err(start_error)?;
    if roots.iter().any(|root| !root.is_dir()) || !roots.iter().any(|root| cwd.starts_with(root)) {
        return Err(UpstreamError::Start(
            "working directory must be contained by an existing workspace root".to_string(),
        ));
    }
    roots
        .into_iter()
        .map(AbsolutePathBuf::from_absolute_path_checked)
        .collect::<Result<Vec<_>, _>>()
        .map_err(start_error)
}

fn canonical_cwd(args: &UpstreamStartArgs) -> Result<PathBuf, UpstreamError> {
    let cwd = std::fs::canonicalize(&args.cwd).map_err(start_error)?;
    if cwd.is_dir() {
        Ok(cwd)
    } else {
        Err(UpstreamError::Start(
            "working directory must be an existing directory".to_string(),
        ))
    }
}

fn start_error(error: impl std::fmt::Display) -> UpstreamError {
    UpstreamError::Start(crate::diagnostics::safe_detail(&error.to_string()))
}
