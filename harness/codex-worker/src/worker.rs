use std::fmt;

use iyw_codex_harness::CodexAcpAgent;
use sacp::{Agent, ConnectTo};

use crate::config::{ConfigError, WorkerConfig};

pub(super) fn run() -> Result<(), WorkerError> {
    let config = WorkerConfig::from_environment().map_err(WorkerError::Configuration)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|_| WorkerError::Runtime)?;
    runtime.block_on(serve(config))
}

async fn serve(config: WorkerConfig) -> Result<(), WorkerError> {
    let agent = CodexAcpAgent::new(config.start_args())
        .map_err(|_| WorkerError::Startup)?
        .with_expected_session_id(config.expected_session_id());
    ConnectTo::<Agent>::connect_to(sacp_tokio::Stdio::new(), agent)
        .await
        .map_err(|_| WorkerError::Protocol)
}

#[derive(Debug)]
pub(super) enum WorkerError {
    Configuration(ConfigError),
    Runtime,
    Startup,
    Protocol,
}

impl fmt::Display for WorkerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration(ConfigError::Directory) => {
                formatter.write_str("worker configuration has an invalid directory")
            }
            Self::Configuration(ConfigError::Executable) => {
                formatter.write_str("worker cannot resolve its executable")
            }
            Self::Configuration(ConfigError::Fingerprint) => {
                formatter.write_str("worker configuration has no runtime fingerprint")
            }
            Self::Runtime => formatter.write_str("worker runtime initialization failed"),
            Self::Startup => formatter.write_str("worker Codex runtime initialization failed"),
            Self::Protocol => formatter.write_str("worker ACP connection failed"),
        }
    }
}
