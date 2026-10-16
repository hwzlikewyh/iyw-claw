//! 仅持久化运行时确认的请求归属；金额始终向 Fusion 账本查询。
use crate::db::{error::DbError, service::app_metadata_service};
use crate::models::agent::AgentType;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const RECEIPT_PREFIX: &str = "runtime.receipts.v1";
const TASK_PREFIX: &str = "runtime.tasks.v1";
const MAX_ID_BYTES: usize = 160;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskReceipt {
    pub thread_id: String,
    pub turn_id: String,
    pub request_id: String,
    pub message_id: String,
    pub parent_tool_use_id: Option<String>,
    pub task_id: Option<String>,
    pub purpose: String,
    #[serde(default)]
    pub root_turn_id: Option<String>,
    #[serde(default)]
    pub tool_ids: Vec<String>,
    #[serde(default)]
    pub goal_identity_known: bool,
}

pub(crate) fn from_sdk(session: &str, message: &Value) -> Option<TaskReceipt> {
    if message.get("type")?.as_str()? != "assistant" {
        return None;
    }
    let request_id = message.get("request_id")?.as_str()?;
    let message_id = message.get("uuid")?.as_str()?;
    let turn_id = message.pointer("/message/id")?.as_str()?;
    if ![session, request_id, message_id, turn_id]
        .into_iter()
        .all(valid_id)
    {
        return None;
    }
    Some(TaskReceipt {
        thread_id: session.to_string(),
        turn_id: turn_id.to_string(),
        request_id: request_id.to_string(),
        message_id: message_id.to_string(),
        task_id: None,
        purpose: "generation".into(),
        root_turn_id: None,
        tool_ids: message
            .pointer("/message/content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|block| block.get("type").and_then(Value::as_str) == Some("tool_use"))
            .filter_map(|block| {
                block
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|id| valid_id(id))
                    .map(str::to_string)
            })
            .collect(),
        parent_tool_use_id: message
            .get("parent_tool_use_id")
            .and_then(Value::as_str)
            .filter(|id| valid_id(id))
            .map(str::to_string),
        goal_identity_known: false,
    })
}

pub(crate) async fn save(
    conn: &DatabaseConnection,
    agent: AgentType,
    receipt: &TaskReceipt,
) -> Result<(), DbError> {
    if ![&receipt.thread_id, &receipt.request_id, &receipt.message_id]
        .into_iter()
        .all(|id| valid_id(id))
    {
        return Err(DbError::Validation(
            "Invalid runtime receipt identity".into(),
        ));
    }
    let key = format!(
        "{}.{}.{}",
        prefix(agent, &receipt.thread_id),
        receipt.request_id,
        receipt.message_id
    );
    let value =
        serde_json::to_string(receipt).map_err(|error| DbError::Validation(error.to_string()))?;
    app_metadata_service::upsert_value(conn, &key, &value).await
}

pub(crate) async fn load(
    conn: &DatabaseConnection,
    agent: AgentType,
    session: &str,
) -> Result<Vec<TaskReceipt>, DbError> {
    let values = app_metadata_service::list_values_by_key_prefix(
        conn,
        &format!("{}.", prefix(agent, session)),
    )
    .await?;
    values
        .into_iter()
        .map(|raw| {
            serde_json::from_str::<TaskReceipt>(&raw).map_err(|error| {
                DbError::Validation(format!("Invalid stored runtime receipt: {error}"))
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|rows| {
            rows.into_iter()
                .filter(|row| row.thread_id == session)
                .collect()
        })
}

pub(crate) async fn save_tasks(
    conn: &DatabaseConnection,
    data: (
        AgentType,
        Option<String>,
        super::runtime_tasks::RuntimeTaskSnapshot,
    ),
) -> Result<(), DbError> {
    let (agent, id, mut snapshot) = data;
    let Some(id) = id else {
        return Ok(());
    };
    for task in &mut snapshot.tasks {
        task.output = None;
    }
    let value =
        serde_json::to_string(&snapshot).map_err(|error| DbError::Validation(error.to_string()))?;
    app_metadata_service::upsert_value(conn, &task_key(agent, &id), &value).await
}

pub(crate) async fn load_tasks(
    conn: &DatabaseConnection,
    agent: AgentType,
    id: &str,
) -> Option<super::runtime_tasks::RuntimeTaskSnapshot> {
    let value = app_metadata_service::get_value(conn, &task_key(agent, id))
        .await
        .ok()??;
    serde_json::from_str(&value).ok()
}

fn prefix(agent: AgentType, id: &str) -> String {
    format!("{RECEIPT_PREFIX}.{agent}.{id}")
}
fn task_key(agent: AgentType, id: &str) -> String {
    format!("{TASK_PREFIX}.{agent}.{id}")
}
fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_ID_BYTES && !value.chars().any(char::is_control)
}
