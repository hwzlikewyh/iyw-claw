//! 新会话配置强制对账（reconciler）。
//!
//! 每次新建 Codex / Claude Code 会话前必须幂等写入并回读受控配置，
//! 生成 fingerprint 并记录诊断；任一必要字段失败必须阻止 spawn。
//! 本模块是 Task 07 的核心，不依赖 `commands/acp.rs` 等共享入口，
//! 由 Task 13 统一接线到各 spawn 路径。

pub mod diagnostics;
pub mod lock;
pub mod merge;
pub mod model;
pub mod write;

use std::path::Path;

use crate::models::agent::AgentType;

pub use diagnostics::{ReconcileDiagnostic, SessionKind};
pub use model::{
    fingerprint_controlled_fields, ManagedFieldKind, ProviderConfigSpec, claude_code_spec,
    codex_spec, session_config_spec_for, SESSION_CONFIG_SCHEMA_VERSION,
};

/// 对账结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconcileOutcome {
    /// 受控字段规范化后的 fingerprint（SHA-256）。
    pub fingerprint: String,
    /// 本次是否实际发生了配置写入（无变化时避免重写但必须回读校验）。
    pub changed: bool,
    /// 受控字段数量。
    pub controlled_fields: usize,
    /// 对账耗时（毫秒）。
    pub duration_ms: u64,
    /// 错误码（`None` 表示成功）；与 `ReconcileDiagnostic.error_code` 同源。
    pub error_code: Option<String>,
}

/// 对账错误：携带稳定错误码，供 UI 诊断与"查看诊断 / 重试 / 打开设置"。
#[derive(Debug, thiserror::Error)]
pub enum ReconcileError {
    #[error("session config reconcile failed: {0}")]
    Failed(String),
    #[error("session config lock timeout: {0}")]
    LockTimeout(String),
    #[error("session config parse failed: {0}")]
    ParseFailed(String),
    #[error("session config read-back verification failed: {0}")]
    VerificationFailed(String),
    #[error("session config write failed: {0}")]
    WriteFailed(String),
}

impl ReconcileError {
    /// 稳定机器可读错误码。
    pub fn code(&self) -> &'static str {
        match self {
            Self::Failed(_) => "session_config_failed",
            Self::LockTimeout(_) => "session_config_lock_timeout",
            Self::ParseFailed(_) => "session_config_parse_failed",
            Self::VerificationFailed(_) => "session_config_verification_failed",
            Self::WriteFailed(_) => "session_config_write_failed",
        }
    }
}

/// 新建会话前必须执行的对账入口（由 Task 13 在 spawn 前接线）。
///
/// `profile_root` 是该 agent 的原生配置目录（Codex: `CODEX_HOME`，
/// Claude Code: `CLAUDE_CONFIG_DIR`）。reconciler 只修改受控字段，
/// 保留用户自定义 MCP 与其他配置；写入失败或回读校验失败都会阻止 spawn。
pub fn reconcile_before_spawn(
    agent: AgentType,
    profile_root: &Path,
) -> Result<ReconcileOutcome, ReconcileError> {
    let spec = session_config_spec_for(agent)?;
    let _guard = lock::acquire_session_lock(agent, profile_root)?;
    let outcome = write::reconcile_managed_files(agent, profile_root, &spec)?;
    diagnostics::record_success(agent, SessionKind::New, &spec, &outcome);
    Ok(outcome)
}

/// 恢复会话对账：保持原策略代际，只刷新允许热更新的安全字段。
pub fn reconcile_resumed_session(
    agent: AgentType,
    profile_root: &Path,
) -> Result<ReconcileOutcome, ReconcileError> {
    let spec = session_config_spec_for(agent)?;
    let _guard = lock::acquire_session_lock(agent, profile_root)?;
    let outcome = write::reconcile_managed_files(agent, profile_root, &spec)?;
    diagnostics::record_success(agent, SessionKind::Resume, &spec, &outcome);
    Ok(outcome)
}
