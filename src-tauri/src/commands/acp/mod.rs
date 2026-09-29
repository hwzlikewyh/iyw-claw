// ACP (Agent Client Protocol) 命令模块
//
// 这个模块已被拆分为多个子模块以提高编译速度和代码可维护性。
// 原始的 12,963 行单文件已重构为按功能划分的模块。

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
#[cfg(feature = "tauri-runtime")]
use tauri::Manager;

use crate::acp::error::AcpError;
use crate::acp::types::{AgentSkillScope, ConfigStaleKind, ConnectionStatus};
use crate::models::agent::AgentType;
use crate::web::event_bridge::EventEmitter;

// 子模块声明
mod runtime_timing;
// mod connection;        // 连接管理
// mod session;           // 会话操作
// mod agent_lifecycle;   // 代理生命周期
// mod agent_config;      // 代理配置
// mod agent_status;      // 代理状态
// mod skills;            // 技能管理
// mod vendor_specific;   // 特定供应商

// 公共常量
const ACP_AGENTS_UPDATED_EVENT: &str = "app://acp-agents-updated";
const CODEX_MODEL_CATALOG_FILE: &str = "iyw-claw-models.json";
const CODEX_MODEL_CONTEXT_WINDOW: u64 = 128_000;

pub(crate) const MANAGED_AGENT_VERSION_ENV: &str = "IYW_CLAW_MANAGED_AGENT_VERSION";

// 事件负载类型
#[derive(Serialize, Clone)]
#[serde(rename_all = "snake_case")]
struct AcpAgentsUpdatedEventPayload {
    reason: &'static str,
    agent_type: Option<AgentType>,
}

/// 发出 ACP 代理更新事件
pub(crate) fn emit_acp_agents_updated(
    emitter: &EventEmitter,
    reason: &'static str,
    agent_type: Option<AgentType>,
) {
    crate::web::event_bridge::emit_event(
        emitter,
        ACP_AGENTS_UPDATED_EVENT,
        AcpAgentsUpdatedEventPayload { reason, agent_type },
    );
}

const AGENT_INSTALL_EVENT: &str = "app://agent-install";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AgentInstallEventKind {
    Started,
    Log,
    Progress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AgentInstallEvent {
    pub task_id: String,
    pub kind: AgentInstallEventKind,
    pub payload: String,
}

pub(crate) fn emit_managed_tool_progress(emitter: &EventEmitter, task_id: &str, percent: u8) {
    emit_agent_install_event(
        emitter,
        task_id,
        AgentInstallEventKind::Progress,
        percent.to_string(),
    );
}

fn emit_agent_install_event(
    emitter: &EventEmitter,
    task_id: &str,
    kind: AgentInstallEventKind,
    payload: String,
) {
    crate::web::event_bridge::emit_event(
        emitter,
        AGENT_INSTALL_EVENT,
        AgentInstallEvent {
            task_id: task_id.to_string(),
            kind,
            payload,
        },
    );
}

// 重新导出所有公共函数（暂时从原文件导入，逐步迁移）
//
// 迁移策略：
// 1. 保持原 acp.rs 文件功能完整
// 2. 逐个模块迁移函数到新文件
// 3. 在 mod.rs 中重新导出
// 4. 最终删除原 acp.rs，将此文件提升为主入口

// 临时：从父模块重新导出（待迁移完成后删除）
// pub use super::acp_old::*;
