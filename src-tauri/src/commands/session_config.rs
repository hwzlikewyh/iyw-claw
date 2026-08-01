//! 新会话配置对账（reconciler）诊断暴露。
//!
//! Task 07 在每次新建/恢复会话前记录脱敏的 `ReconcileDiagnostic`（进程内
//! 环形缓冲，不含配置正文/token/key/完整用户路径）。本模块把它暴露给
//! 设置页，用于展示"最近一次对账时间/结果"与错误码（查看诊断/重试/打开
//! 设置入口的数据源）。命令与 web handler 共享同一 core，避免桌面与
//! server runtime 行为分叉。

use serde::Serialize;

use crate::acp::session_config_reconciler::diagnostics::{diagnostics_snapshot, ReconcileDiagnostic};
use crate::app_error::AppCommandError;
use crate::models::agent::AgentType;

/// 对账诊断快照响应：最近在前 + 各受管 agent 的最近一条。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionConfigReconcileDiagnostics {
    pub codex: Option<ReconcileDiagnostic>,
    pub claude_code: Option<ReconcileDiagnostic>,
    pub recent: Vec<ReconcileDiagnostic>,
}

/// 读取当前进程内的对账诊断快照（Tauri command 与 web handler 共用）。
pub fn session_config_reconcile_diagnostics_core() -> SessionConfigReconcileDiagnostics {
    let recent = diagnostics_snapshot();
    let last_for = |agent: AgentType| recent.iter().find(|item| item.agent == agent).cloned();
    SessionConfigReconcileDiagnostics {
        codex: last_for(AgentType::Codex),
        claude_code: last_for(AgentType::ClaudeCode),
        recent,
    }
}

/// Tauri command：设置页读取最近一次对账时间/结果。
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn get_session_config_reconcile_diagnostics() -> Result<SessionConfigReconcileDiagnostics, AppCommandError> {
    Ok(session_config_reconcile_diagnostics_core())
}
