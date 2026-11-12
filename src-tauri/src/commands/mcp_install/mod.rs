use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, Mutex};

use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::app_error::AppCommandError;

pub(crate) mod progress;
mod store;
#[cfg(feature = "tauri-runtime")]
mod transport;
#[cfg(feature = "tauri-runtime")]
pub use transport::*;

const MAX_ACTIVE: usize = 8;
const MAX_PARAMETER_BYTES: usize = 1024 * 1024;
const MAX_PARAMETERS: usize = 64;
static TASKS: LazyLock<Mutex<BTreeMap<String, Work>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallTask {
    pub id: String,
    pub server_id: String,
    pub option_id: Option<String>,
    pub phase: String,
    pub received_bytes: u64,
    pub total_bytes: Option<u64>,
    pub cancellable: bool,
    pub error: Option<String>,
    pub created_at: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallInput {
    pub provider_id: String,
    pub server_id: String,
    pub option_id: Option<String>,
    pub protocol: Option<String>,
    pub parameter_values: Option<Value>,
}

#[derive(Clone)]
pub(super) struct Work {
    view: Arc<Mutex<InstallTask>>,
    cancel: CancellationToken,
    done: CancellationToken,
}

fn active(phase: &str) -> bool {
    !matches!(phase, "completed" | "failed" | "cancelled" | "interrupted")
}

pub async fn begin(
    conn: &DatabaseConnection,
    input: InstallInput,
) -> Result<InstallTask, AppCommandError> {
    validate_input(&input)?;
    let storage_guard = crate::acp::agent_storage_work::begin_agent_storage_read().await;
    super::mcp::require_private_agent_storage_for_write()?;
    let view = InstallTask {
        id: uuid::Uuid::new_v4().to_string(),
        server_id: input.server_id.clone(),
        option_id: input.option_id.clone(),
        phase: "preparing".into(),
        received_bytes: 0,
        total_bytes: None,
        cancellable: true,
        error: None,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    let work = reserve(view.clone())?;
    if let Err(error) = store::save(conn, view.clone()).await {
        if let Ok(mut tasks) = TASKS.lock() {
            tasks.remove(&view.id);
        }
        return Err(error);
    }
    let conn = conn.clone();
    tokio::spawn(async move {
        let _storage_guard = storage_guard;
        run(conn, input, work).await;
    });
    Ok(view)
}

fn validate_input(input: &InstallInput) -> Result<(), AppCommandError> {
    let valid_id = |value: &str| {
        !value.is_empty()
            && value.len() <= 96
            && value
                .bytes()
                .all(|value| value.is_ascii_alphanumeric() || b"._-".contains(&value))
    };
    if input.provider_id != "iyw_gateway"
        || !valid_id(&input.server_id)
        || input
            .option_id
            .as_deref()
            .is_some_and(|value| !valid_id(value))
    {
        return Err(AppCommandError::invalid_input("连接器市场或标识无效"));
    }
    if input
        .protocol
        .as_deref()
        .is_some_and(|value| !matches!(value, "stdio" | "http" | "sse"))
    {
        return Err(AppCommandError::invalid_input("连接器传输方式无效"));
    }
    if let Some(values) = &input.parameter_values {
        let fields = values
            .as_object()
            .ok_or_else(|| AppCommandError::invalid_input("连接器参数必须是对象"))?;
        if fields.len() > MAX_PARAMETERS
            || serde_json::to_vec(values)
                .map_err(|_| AppCommandError::invalid_input("连接器参数无效"))?
                .len()
                > MAX_PARAMETER_BYTES
        {
            return Err(AppCommandError::invalid_input(
                "连接器参数超过大小或数量限制",
            ));
        }
    }
    Ok(())
}

fn reserve(view: InstallTask) -> Result<Work, AppCommandError> {
    let mut tasks = TASKS
        .lock()
        .map_err(|_| AppCommandError::configuration_invalid("安装任务不可用"))?;
    let running = tasks
        .values()
        .filter(|work| {
            work.view
                .lock()
                .map(|value| active(&value.phase))
                .unwrap_or(true)
        })
        .count();
    if running >= MAX_ACTIVE
        || tasks.values().any(|work| {
            work.view
                .lock()
                .map(|value| active(&value.phase) && value.server_id == view.server_id)
                .unwrap_or(true)
        })
    {
        return Err(AppCommandError::invalid_input(
            "此连接器已有安装任务，或并行任务已满，请先完成或取消",
        ));
    }
    prune(&mut tasks);
    let work = Work {
        view: Arc::new(Mutex::new(view.clone())),
        cancel: CancellationToken::new(),
        done: CancellationToken::new(),
    };
    tasks.insert(view.id, work.clone());
    Ok(work)
}

fn prune(tasks: &mut BTreeMap<String, Work>) {
    const MAX_MEMORY_TASKS: usize = 64;
    if tasks.len() < MAX_MEMORY_TASKS {
        return;
    }
    let oldest = tasks
        .iter()
        .filter_map(|(id, work)| {
            work.view
                .lock()
                .ok()
                .filter(|view| !active(&view.phase))
                .map(|view| (id.clone(), view.created_at.clone()))
        })
        .min_by(|left, right| left.1.cmp(&right.1));
    if let Some((id, _)) = oldest {
        tasks.remove(&id);
    }
}

async fn run(conn: DatabaseConnection, input: InstallInput, work: Work) {
    let result = progress::CURRENT
        .scope(
            work.clone(),
            super::mcp::mcp_install_from_marketplace_core(
                &conn,
                input.provider_id,
                input.server_id.clone(),
                None,
                input.option_id,
                input.protocol,
                input.parameter_values,
            ),
        )
        .await;
    let view = {
        let Ok(mut view) = work.view.lock() else {
            return;
        };
        view.phase = if result.is_ok() {
            "completed"
        } else if work.cancel.is_cancelled() {
            "cancelled"
        } else {
            "failed"
        }
        .into();
        view.cancellable = false;
        view.error = result.err().map(|error| error.message);
        view.clone()
    };
    tracing::info!(task_id=%view.id, connector=%view.server_id, phase=%view.phase, "[MCP] 连接器安装任务结束");
    if let Err(error) = store::save(&conn, view).await {
        tracing::error!(code=?error.code, "[MCP] 无法保存安装任务结果，请核对已安装目录");
    }
    work.done.cancel();
}

pub async fn wait_for_completion(id: &str) {
    let work = TASKS.lock().ok().and_then(|tasks| tasks.get(id).cloned());
    let Some(work) = work else { return };
    work.done.cancelled().await;
}

pub async fn list(conn: &DatabaseConnection) -> Result<Vec<InstallTask>, AppCommandError> {
    let mut entries = store::read(conn).await?;
    project_entries(&mut entries)?;
    entries.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    Ok(entries)
}

fn project_entries(entries: &mut [InstallTask]) -> Result<(), AppCommandError> {
    let tasks = TASKS
        .lock()
        .map_err(|_| AppCommandError::configuration_invalid("安装任务不可用"))?;
    for entry in entries {
        if let Some(work) = tasks.get(&entry.id) {
            *entry = work
                .view
                .lock()
                .map_err(|_| AppCommandError::configuration_invalid("安装任务不可用"))?
                .clone();
        } else if active(&entry.phase) {
            entry.phase = "interrupted".into();
            entry.cancellable = false;
            entry.error = Some("应用曾在安装过程中退出，请核对已安装项；重新填写所需参数后可重试，已校验的文件会复用".into());
        }
    }
    Ok(())
}

pub async fn status(conn: &DatabaseConnection, id: &str) -> Result<InstallTask, AppCommandError> {
    list(conn)
        .await?
        .into_iter()
        .find(|view| view.id == id)
        .ok_or_else(|| AppCommandError::not_found("安装任务记录已过期，请刷新已安装列表"))
}

pub async fn cancel(conn: &DatabaseConnection, id: &str) -> Result<InstallTask, AppCommandError> {
    let view = status(conn, id).await?;
    if view.cancellable {
        if let Ok(tasks) = TASKS.lock() {
            if let Some(work) = tasks.get(id) {
                let mut current = work
                    .view
                    .lock()
                    .map_err(|_| AppCommandError::configuration_invalid("安装任务不可用"))?;
                if current.cancellable {
                    work.cancel.cancel();
                    current.phase = "cancelling".into();
                }
            }
        }
    }
    status(conn, id).await
}
