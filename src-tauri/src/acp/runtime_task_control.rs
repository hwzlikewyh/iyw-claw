use std::sync::Arc;
use std::time::Duration;

use sacp::{Agent, ConnectionTo, UntypedMessage};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::RwLock;

use super::{RuntimeTask, RuntimeTaskSnapshot};
use crate::acp::{error::AcpError, manager::ConnectionManager, session_state::SessionState};
use crate::models::agent::AgentType;
use crate::web::event_bridge::EventEmitter;

const CONTROL_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TaskControl {
    pub task_id: String,
    pub native_id: String,
    pub action: String,
}

impl ConnectionManager {
    pub async fn control_runtime_task(
        &self,
        id: &str,
        request: TaskControl,
    ) -> Result<RuntimeTaskSnapshot, AcpError> {
        let _operation = self.acquire_operation_read().await?;
        if request.task_id.starts_with("delegation:") {
            drop(_operation);
            self.stop_runtime_delegation(id, &request).await?;
            return self.runtime_task_snapshot(id).await;
        }
        let sender = self
            .connections
            .lock()
            .await
            .get(id)
            .ok_or_else(|| AcpError::ConnectionNotFound(id.into()))?
            .cmd_tx
            .clone();
        let (reply, response) = tokio::sync::oneshot::channel();
        sender
            .send(crate::acp::connection::ConnectionCommand::RuntimeTaskControl { request, reply })
            .await
            .map_err(|_| AcpError::ProcessExited)?;
        response.await.map_err(|_| AcpError::ProcessExited)??;
        self.runtime_task_snapshot(id).await
    }

    async fn stop_runtime_delegation(
        &self,
        id: &str,
        request: &TaskControl,
    ) -> Result<(), AcpError> {
        if request.action != "stop" {
            return Err(AcpError::protocol("Unsupported delegated task action"));
        }
        let tool = request
            .task_id
            .strip_prefix("delegation:")
            .ok_or_else(|| AcpError::protocol("Invalid delegated task identity"))?;
        let state = self
            .get_state(id)
            .await
            .ok_or_else(|| AcpError::ConnectionNotFound(id.into()))?;
        let child = state
            .read()
            .await
            .active_delegations
            .get(tool)
            .cloned()
            .ok_or_else(|| AcpError::protocol("Delegated task is no longer running"))?;
        if child.child_connection_id != request.native_id {
            return Err(AcpError::protocol(
                "Task changed; synchronize before controlling it",
            ));
        }
        let db = self
            .agent_input_runtime
            .db()
            .ok_or_else(|| AcpError::protocol("Task database is unavailable"))?;
        self.cancel(&db.conn, &child.child_connection_id).await
    }
}

pub(crate) async fn execute(
    context: (&ConnectionTo<Agent>, &str, AgentType),
    state: (&Arc<RwLock<SessionState>>, &EventEmitter),
    request: TaskControl,
) -> Result<(), AcpError> {
    let (connection, session_id, agent) = context;
    let task = state
        .0
        .read()
        .await
        .runtime_tasks
        .get(&request.task_id)
        .cloned()
        .ok_or_else(|| AcpError::protocol("Task is no longer available"))?;
    if !task.actions.contains(&request.action) {
        return Err(AcpError::protocol(
            "The runtime does not support this task action",
        ));
    }
    if task.native_id != request.native_id {
        return Err(AcpError::protocol(
            "Task changed; synchronize before controlling it",
        ));
    }
    let (method, params) = control_request(agent, session_id, (&task, &request.action))?;
    tracing::info!(task_id = %task.id, action = %request.action, method, "[runtime-task] control requested");
    send_control(connection, (method, params)).await.map_err(|error| {
        tracing::warn!(task_id = %task.id, action = %request.action, %error, "[runtime-task] control failed or unconfirmed");
        error
    })?;
    let current = state.0.read().await;
    validate_reply(&current, session_id, &task)?;
    // 请求回执不覆盖原生生命周期事件，避免旧响应写回新的目标或会话。
    drop(current);
    if agent == AgentType::ClaudeCode && task.kind == "goal" {
        super::events::await_goal_confirmation(state, (session_id, &task)).await;
    }
    tracing::info!(task_id = %task.id, action = %request.action, "[runtime-task] control acknowledged");
    Ok(())
}

fn validate_reply(
    current: &SessionState,
    session_id: &str,
    task: &RuntimeTask,
) -> Result<(), AcpError> {
    if current.external_id.as_deref() != Some(session_id)
        || current
            .runtime_tasks
            .get(&task.id)
            .is_none_or(|current| current.native_id != task.native_id)
    {
        return Err(AcpError::protocol(
            "Task changed while control was in flight; synchronize status",
        ));
    }
    Ok(())
}

async fn send_control(
    connection: &ConnectionTo<Agent>,
    rpc: (&str, Value),
) -> Result<Value, AcpError> {
    let (method, params) = rpc;
    let rpc = UntypedMessage::new(method, params)
        .map_err(|error| AcpError::protocol(error.to_string()))?;
    let result = tokio::time::timeout(
        CONTROL_TIMEOUT,
        connection.send_request_to(Agent, rpc).block_task(),
    )
    .await
    .map_err(|_| AcpError::protocol("Task control timed out; synchronize status before retrying"))?
    .map_err(|error| AcpError::protocol(error.to_string()))?;
    if method == "_session/async_task/stop" && result.get("stopped") != Some(&Value::Bool(true)) {
        return Err(AcpError::protocol(
            "The runtime did not confirm stopping this task",
        ));
    }
    Ok(result)
}

fn control_request(
    agent: AgentType,
    session: &str,
    action: (&RuntimeTask, &str),
) -> Result<(&'static str, Value), AcpError> {
    let (task, action) = action;
    match (agent, task.kind.as_str(), action) {
        (AgentType::Codex, "goal", "pause" | "resume") => Ok((
            "thread/goal/set",
            json!({
                "sessionId": session, "status": if action == "pause" { "paused" } else { "active" }
            }),
        )),
        (AgentType::Codex, "goal", "clear") => {
            Ok(("thread/goal/clear", json!({"sessionId": session})))
        }
        (AgentType::Codex, "subagent", "stop") => Ok((
            "_session/task/stop",
            json!({"sessionId": session, "threadId": task.native_id}),
        )),
        (AgentType::ClaudeCode, "goal", "clear") => Ok((
            "_session/goal",
            json!({"sessionId": session, "action": "clear"}),
        )),
        (AgentType::ClaudeCode, "background", "stop") => Ok((
            "_session/async_task/stop",
            json!({"sessionId": session, "asyncTaskId": task.native_id}),
        )),
        _ => Err(AcpError::protocol("Unsupported native task control")),
    }
}
