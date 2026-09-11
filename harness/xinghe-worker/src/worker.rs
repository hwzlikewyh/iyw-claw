use std::fmt;

use iyw_codex_harness::CodexAcpAgent;
use sacp::{Client, ConnectTo};

use crate::config::{ConfigError, WorkerConfig};
use crate::diagnostics::{safe_detail, StartupStage};

pub(super) fn run() -> Result<(), WorkerError> {
    let config = StartupStage::new("worker_config")
        .finish(WorkerConfig::from_environment().map_err(WorkerError::Configuration))
        .map_err(WorkerError::Startup)?;
    let runtime = StartupStage::new("async_runtime")
        .finish(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build(),
        )
        .map_err(WorkerError::Runtime)?;
    runtime.block_on(serve(config))
}

async fn serve(config: WorkerConfig) -> Result<(), WorkerError> {
    let agent = StartupStage::new("acp_facade")
        .finish(CodexAcpAgent::new(config.start_args()))
        .map_err(WorkerError::Startup)?
        .with_owner(config.connection_id(), None, 0)
        .map_err(|_| WorkerError::Configuration(ConfigError::Connection))?
        .with_expected_session_id(config.expected_session_id());
    // 先加载运行器配置，避免启动失败后 Runtime 析构等待阻塞的 stdin 读取。
    ConnectTo::<Client>::connect_to(agent, sacp_tokio::Stdio::new())
        .await
        .map_err(|error| WorkerError::Protocol(safe_detail(&error.to_string())))
}

#[derive(Debug)]
pub(super) enum WorkerError {
    Configuration(ConfigError),
    Runtime(String),
    Startup(String),
    Protocol(String),
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
            Self::Configuration(ConfigError::Connection) => {
                formatter.write_str("worker configuration has no owning connection")
            }
            Self::Runtime(detail) => {
                write!(formatter, "worker runtime initialization failed: {detail}")
            }
            Self::Startup(detail) => write!(formatter, "内置星河运行时初始化失败: {detail}"),
            Self::Protocol(detail) => write!(formatter, "worker ACP connection failed: {detail}"),
        }
    }
}
