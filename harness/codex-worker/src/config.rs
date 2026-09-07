use std::path::PathBuf;

use iyw_codex_harness::{Capability, CapabilitySet, HarnessConfig, UpstreamStartArgs};

const HOME_ENV: &str = "IYW_CLAW_CODEX_WORKER_HOME";
const CWD_ENV: &str = "IYW_CLAW_CODEX_WORKER_CWD";
const FINGERPRINT_ENV: &str = "IYW_CLAW_CODEX_WORKER_FINGERPRINT";
const SESSION_ENV: &str = "IYW_CLAW_CODEX_WORKER_EXPECTED_SESSION_ID";
const CONNECTION_ENV: &str = "IYW_CLAW_CODEX_WORKER_CONNECTION_ID";

pub(super) struct WorkerConfig {
    connection_id: String,
    codex_home: PathBuf,
    cwd: PathBuf,
    runtime_fingerprint: String,
    expected_session_id: Option<String>,
    helper_executable: PathBuf,
}

impl WorkerConfig {
    pub(super) fn from_environment() -> Result<Self, ConfigError> {
        Ok(Self {
            connection_id: optional_value(CONNECTION_ENV).ok_or(ConfigError::Connection)?,
            codex_home: required_directory(HOME_ENV)?,
            cwd: required_directory(CWD_ENV)?,
            runtime_fingerprint: required_value(FINGERPRINT_ENV)?,
            expected_session_id: optional_value(SESSION_ENV),
            helper_executable: helper_executable()?,
        })
    }

    pub(super) fn start_args(&self) -> UpstreamStartArgs {
        UpstreamStartArgs {
            harness: HarnessConfig {
                experimental_api: true,
                ..Default::default()
            },
            runtime_fingerprint: self.runtime_fingerprint.clone(),
            capabilities: worker_capabilities(),
            codex_home: self.codex_home.clone(),
            cwd: self.cwd.clone(),
            workspace_roots: vec![self.cwd.clone()],
            helper_executable: self.helper_executable.clone(),
            linux_sandbox_executable: cfg!(target_os = "linux")
                .then(|| self.helper_executable.clone()),
            main_execve_wrapper_executable: None,
            enable_codex_api_key_env: false,
            mcp_server_openai_form_elicitation: false,
            opt_out_notification_methods: Vec::new(),
        }
    }

    pub(super) fn expected_session_id(&self) -> Option<String> {
        self.expected_session_id.clone()
    }

    pub(super) fn connection_id(&self) -> &str {
        &self.connection_id
    }
}

fn helper_executable() -> Result<PathBuf, ConfigError> {
    let path = std::env::var_os("IYW_CLAW_CODEX_WORKER_HELPER")
        .map(PathBuf::from)
        .ok_or(ConfigError::Executable)?;
    (path.is_absolute() && path.is_file())
        .then_some(path)
        .ok_or(ConfigError::Executable)
}

#[derive(Debug)]
pub(super) enum ConfigError {
    Directory,
    Executable,
    Fingerprint,
    Connection,
}

fn required_directory(name: &str) -> Result<PathBuf, ConfigError> {
    let path = std::env::var_os(name)
        .map(PathBuf::from)
        .ok_or(ConfigError::Directory)?;
    (path.is_absolute() && path.is_dir())
        .then_some(path)
        .ok_or(ConfigError::Directory)
}

fn required_value(name: &str) -> Result<String, ConfigError> {
    optional_value(name).ok_or(ConfigError::Fingerprint)
}

fn optional_value(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn worker_capabilities() -> CapabilitySet {
    CapabilitySet::empty()
        .with(Capability::Prompt)
        .with(Capability::Cancellation)
        .with(Capability::Steering)
        .with(Capability::Images)
        .with(Capability::Permission)
        .with(Capability::Mcp)
        .with(Capability::Subagents)
        .with(Capability::Skills)
        .with(Capability::Goals)
        .with(Capability::Configuration)
}
