//! Per-channel readiness evaluation.
//!
//! Readiness answers "can this channel actually complete a conversation?" as
//! a staged state machine:
//!
//! ```text
//! saved -> credential_ready -> transport_connected -> inbound_verified
//!       -> workspace_ready -> agent_ready -> roundtrip_ready
//! ```
//!
//! Checking readiness never executes user tasks: it inspects config,
//! credentials, the live transport, the workspace directory and agent
//! installation state only.

use sea_orm::DatabaseConnection;
use serde::Serialize;

use super::config_patch::parse_config_object;
use super::manager::ChatChannelManager;
use super::reconcile;
use crate::acp::agent_storage::AgentStoragePaths;
use crate::db::entities::chat_channel;
use crate::db::service::{chat_channel_service, folder_service};
use crate::models::agent::AgentType;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadinessStage {
    /// Machine key: saved | credential | transport | inbound | workspace |
    /// agent | gateway | roundtrip.
    pub key: String,
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelReadinessReport {
    pub channel_id: i32,
    pub name: String,
    pub channel_type: String,
    /// Desired state from the DB (never conflated with connectivity).
    pub enabled: bool,
    /// Last persisted runtime state (from the reconcile path).
    pub runtime_status: String,
    /// Live transport state from the manager.
    pub transport_connected: bool,
    pub saved: bool,
    pub credential_ready: bool,
    pub inbound_verified: bool,
    pub workspace_ready: bool,
    pub agent_ready: bool,
    pub gateway_ready: bool,
    pub roundtrip_ready: bool,
    /// First failing stage key, e.g. `credential` — every failure maps to a
    /// readiness stage.
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub last_error: Option<String>,
    pub last_error_at: Option<String>,
    pub last_connected_at: Option<String>,
    pub last_inbound_at: Option<String>,
    pub inbound_count: u64,
    pub stages: Vec<ReadinessStage>,
}

pub async fn evaluate_readiness(
    db: &DatabaseConnection,
    manager: &ChatChannelManager,
    model: &chat_channel::Model,
) -> ChannelReadinessReport {
    let mut stages: Vec<ReadinessStage> = Vec::new();
    let mut push = |key: &str, ok: bool, error: Option<String>| {
        stages.push(ReadinessStage {
            key: key.to_string(),
            ok,
            error,
        });
    };

    // saved
    let saved = parse_config_object(&model.config_json).is_ok();
    push(
        "saved",
        saved,
        if saved {
            None
        } else {
            Some("渠道配置无法解析，请重新保存配置修复".to_string())
        },
    );

    // credential
    let credential = match reconcile::credential_ready(db, model).await {
        Ok(()) => (true, None),
        Err(message) => (false, Some(message)),
    };
    push("credential", credential.0, credential.1.clone());

    // transport
    let transport_connected = manager.is_connected(model.id).await;
    push(
        "transport",
        transport_connected,
        if transport_connected {
            None
        } else {
            Some("渠道尚未连接".to_string())
        },
    );

    // inbound (requires a live transport that has actually delivered a message)
    let (last_inbound_at, inbound_count) = manager.inbound_stats(model.id).await;
    let inbound_verified = transport_connected && inbound_count > 0;
    let inbound_error = if transport_connected && !inbound_verified {
        Some("已连接但尚未收到任何消息".to_string())
    } else {
        None
    };
    push("inbound", inbound_verified, inbound_error);

    // workspace
    let workspace = check_workspace(model);
    push("workspace", workspace.0, workspace.1);

    // agent
    let agent = check_agent(db, model).await;
    push("agent", agent.0, agent.1.clone());

    // gateway (managed model gateway availability for routing/execution)
    let gateway = check_gateway(db, model).await;
    push("gateway", gateway.0, gateway.1.clone());

    // roundtrip: all stages above green
    let roundtrip_ready = saved
        && credential.0
        && transport_connected
        && inbound_verified
        && workspace.0
        && agent.0
        && gateway.0;
    push(
        "roundtrip",
        roundtrip_ready,
        if roundtrip_ready {
            None
        } else {
            Some("尚不满足完整回环条件".to_string())
        },
    );

    let first_failure = stages.iter().find(|s| !s.ok);
    let error_code = first_failure.map(|s| s.key.clone());
    let error_message = first_failure.and_then(|s| s.error.clone());

    ChannelReadinessReport {
        channel_id: model.id,
        name: model.name.clone(),
        channel_type: model.channel_type.clone(),
        enabled: model.enabled,
        runtime_status: model.runtime_status.clone(),
        transport_connected,
        saved,
        credential_ready: credential.0,
        inbound_verified,
        workspace_ready: workspace.0,
        agent_ready: agent.0,
        gateway_ready: gateway.0,
        roundtrip_ready,
        error_code,
        error_message,
        last_error: model.last_error.clone(),
        last_error_at: model.last_error_at.map(|v| v.to_rfc3339()),
        last_connected_at: model.last_connected_at.map(|v| v.to_rfc3339()),
        last_inbound_at: last_inbound_at.map(|v| v.to_rfc3339()),
        inbound_count,
        stages,
    }
}

/// Evaluate readiness for every channel (list view).
pub async fn evaluate_all(
    db: &DatabaseConnection,
    manager: &ChatChannelManager,
) -> Vec<ChannelReadinessReport> {
    let models = match chat_channel_service::list_all(db).await {
        Ok(models) => models,
        Err(e) => {
            tracing::error!("[readiness] list_all failed: {e}");
            return Vec::new();
        }
    };
    let mut reports = Vec::with_capacity(models.len());
    for model in models {
        if model.channel_type == super::types::ChannelType::WecomAgent.to_string() {
            continue;
        }
        reports.push(evaluate_readiness(db, manager, &model).await);
    }
    reports
}

/// `channel_workspace_root` must exist and the daily folder must be creatable
/// (the same operation the natural router performs at routing time).
fn check_workspace(model: &chat_channel::Model) -> (bool, Option<String>) {
    let config = match parse_config_object(&model.config_json) {
        Ok(map) => map,
        Err(e) => return (false, Some(e)),
    };
    let Some(root) = config
        .get("channel_workspace_root")
        .and_then(|v| v.as_str())
    else {
        // Legacy channels without a workspace root are still usable; treat the
        // workspace stage as informational rather than blocking.
        return (true, None);
    };
    let root_path = std::path::PathBuf::from(root);
    if let Err(e) = std::fs::create_dir_all(&root_path) {
        return (false, Some(format!("无法创建渠道工作区 {root}: {e}")));
    }
    let today_path = root_path.join(chrono::Local::now().format("%Y-%m-%d").to_string());
    if let Err(e) = std::fs::create_dir_all(&today_path) {
        return (
            false,
            Some(format!("渠道工作区不可写（{today_path:?}）：{e}")),
        );
    }
    (true, None)
}

/// Default agent for a channel must resolve, be enabled, installed, and the
/// agent storage must be active.
async fn check_agent(
    db: &DatabaseConnection,
    model: &chat_channel::Model,
) -> (bool, Option<String>) {
    if AgentStoragePaths::active().is_none() {
        return (false, Some("Agent 存储尚未初始化".to_string()));
    }

    let agent_type = resolve_default_agent(db, model).await;
    let Some(agent_type) = agent_type else {
        return (false, Some("无法解析默认 Agent".to_string()));
    };

    match crate::commands::acp::acp_get_agent_status_core(
        agent_type,
        &crate::db::AppDatabase { conn: db.clone() },
    )
    .await
    {
        Ok(status) => {
            if !status.enabled {
                return (false, Some(format!("Agent {} 未启用", agent_type)));
            }
            if status.installed_version.is_none() {
                return (false, Some(format!("Agent {} 未安装", agent_type)));
            }
            if !status.available {
                return (false, Some(format!("Agent {} 不可用", agent_type)));
            }
            (true, None)
        }
        Err(error) => (false, Some(format!("Agent 状态检查失败：{error}"))),
    }
}

/// Channel default agent → folder default → Codex, mirroring the dispatcher's
/// resolution chain for a fresh sender.
async fn resolve_default_agent(
    db: &DatabaseConnection,
    model: &chat_channel::Model,
) -> Option<AgentType> {
    let config = parse_config_object(&model.config_json).ok()?;
    if let Some(value) = config.get("default_agent_type").and_then(|v| v.as_str()) {
        if let Ok(agent) = serde_json::from_value(serde_json::Value::String(value.to_string())) {
            return Some(agent);
        }
    }
    if let Some(folder_id) = config.get("default_folder_id").and_then(|v| v.as_i64()) {
        if let Ok(Some(folder)) = folder_service::get_folder_by_id(db, folder_id as i32).await {
            if let Some(default) = folder.default_agent_type {
                return Some(default);
            }
        }
    }
    Some(AgentType::Codex)
}

/// Managed gateway availability: the built-in model gateway rides the iyw
/// account token (same signal the natural router uses).
async fn check_gateway(
    db: &DatabaseConnection,
    _model: &chat_channel::Model,
) -> (bool, Option<String>) {
    match crate::commands::iyw_account::iyw_account_access_token_core(db).await {
        Ok(Some(_)) => (true, None),
        Ok(None) => (false, Some("未登录 iyw 账号，模型网关不可用".to_string())),
        Err(error) => (false, Some(format!("模型网关配置不可用：{error}"))),
    }
}
