use rmcp::model::CallToolResult;
use rmcp::ErrorData;
use serde_json::{json, Value};

use crate::chat_channel::task_context::{self, TaskContext};
use crate::chat_channel::types::ChannelType;

const PREFIX: &str = "iyw.channel.";

pub(super) struct ChannelRequest {
    pub task: TaskContext,
    pub operation: String,
    pub arguments: Value,
}

fn entries(task: &TaskContext) -> Vec<Value> {
    let mut entries = vec![
        entry(
            task,
            "context",
            ("查看当前渠道和任务的真实上下文", json!({})),
        ),
        entry(
            task,
            "progress",
            ("查看当前长任务的真实执行进度", scoped(json!({}))),
        ),
        entry(
            task,
            "reporting",
            (
                "设置本任务定时汇报；停止汇报不会取消任务",
                scoped(json!({
                    "interval_seconds": {"type":"integer","minimum":30,"maximum":86400},
                    "enabled":{"type":"boolean"}
                })),
            ),
        ),
        entry(
            task,
            "milestone",
            (
                "记录用户可见的重要进展，由宿主汇总通知；不要包含秘密或内部推理",
                scoped(json!({
                    "summary":{"type":"string","minLength":1,"maxLength":500}
                })),
            ),
        ),
        entry(
            task,
            "send",
            (
                "向当前聊天发送文字、图片或文件，账号和收件人由宿主绑定；检查每项结果",
                scoped(json!({
                    "request_id":{"type":"string","minLength":1,"maxLength":128},
                    "text":{"type":"string","minLength":1},
                    "files":{"type":"array","items":{"type":"string"},"maxItems":10}
                })),
            ),
        ),
        card_entry(task),
    ];
    wecom_entries(task, &mut entries);
    entries
}

fn wecom_entries(task: &TaskContext, entries: &mut Vec<Value>) {
    if task.origin.channel_type == ChannelType::WecomAiBot {
        entries.push(entry(task, "stream", ("在当前企业微信聊天创建或刷新流式消息，宿主管理 ID 并在回合结束或九分钟时收尾；普通进度由宿主独立发送", scoped(json!({
            "request_id":{"type":"string","minLength":1,"maxLength":128},
            "content":{"type":"string","minLength":1,"maxLength":20480},
            "finished":{"type":"boolean","default":false}
        }))),));
        entries.push(entry(task, "business", ("通过当前企业微信账号的受管 CLI 操作文档、智能表格、日程等业务；先用 help 读取精确命令，凭据由宿主隔离", scoped(json!({
            "request_id":{"type":"string","minLength":1,"maxLength":128},
            "command":{"type":"array","minItems":1,"maxItems":6,"items":{"type":"string","pattern":"^[a-z][a-z0-9-]*$"}},
            "parameters":{"type":"object"}, "help":{"type":"boolean","default":false},
            "schema":{"type":"boolean","default":false,"description":"Inspect exact method parameters before executing."}
        }))),));
    }
}

fn scoped(mut properties: Value) -> Value {
    properties["context_id"] = json!({"type":"string","description":"Copy context_id from this session's channel context; never use a different task's value."});
    properties
}

fn entry(task: &TaskContext, operation: &str, detail: (&str, Value)) -> Value {
    let (description, properties) = detail;
    let required = match operation {
        "context" => vec![],
        "send" => vec!["context_id", "request_id"],
        "stream" => vec!["context_id", "request_id", "content"],
        "milestone" => vec!["context_id", "summary"],
        "card" => vec!["context_id", "request_id", "title", "body"],
        "business" => vec!["context_id", "request_id", "command"],
        _ => vec!["context_id"],
    };
    json!({
        "capability_id": format!("{PREFIX}{}.{operation}.v1", task.origin.channel_type),
        "summary": description, "description": description, "category":"current_channel",
        "name": format!("{} · {description}", task.origin.channel_type),
        "status":"available", "context_id":task.id,
        "input_schema":{"type":"object","properties":properties,"required":required,"additionalProperties":false},
    })
}

pub(super) fn search(connection_id: &str, query: &str) -> Vec<Value> {
    let Some(task) = task_context::capability(connection_id) else {
        return vec![];
    };
    let query = query.to_lowercase();
    let explicit = [
        "渠道", "微信", "企微", "飞书", "钉钉", "channel", "wecom", "weixin", "wechat", "lark",
        "dingtalk",
    ]
    .iter()
    .any(|term| query.contains(term));
    let mut values = entries(&task)
        .into_iter()
        .filter(|entry| {
            explicit
                || operation_matches(entry["capability_id"].as_str().unwrap_or_default(), &query)
        })
        .collect::<Vec<_>>();
    values.sort_by_key(|entry| {
        let text = format!("{} {}", entry["description"], entry["capability_id"]).to_lowercase();
        std::cmp::Reverse(
            query
                .split_whitespace()
                .filter(|term| text.contains(term))
                .count(),
        )
    });
    values
}

pub(super) fn read(connection_id: &str, id: &str) -> Option<Result<Value, ErrorData>> {
    if !id.starts_with(PREFIX) {
        return None;
    }
    Some(
        task_context::capability(connection_id)
            .and_then(|task| {
                entries(&task)
                    .into_iter()
                    .find(|entry| entry["capability_id"] == id)
            })
            .ok_or_else(unavailable),
    )
}

pub(super) fn resolve(
    connection_id: &str,
    id: &str,
    arguments: Value,
) -> Option<Result<ChannelRequest, ErrorData>> {
    let detail = read(connection_id, id)?;
    Some((|| {
        let detail = detail?;
        super::capability_schema::validate(&detail["input_schema"], &arguments)
            .map_err(|error| ErrorData::invalid_params(error.to_string(), None))?;
        let task = task_context::current(connection_id).ok_or_else(unavailable)?;
        if arguments
            .get("context_id")
            .is_some_and(|value| value != &json!(task.id))
        {
            return Err(unavailable());
        }
        let operation = id
            .strip_suffix(".v1")
            .and_then(|id| id.rsplit('.').next())
            .ok_or_else(unavailable)?;
        Ok(ChannelRequest {
            task,
            operation: operation.into(),
            arguments,
        })
    })())
}

pub(super) async fn execute(
    service: &crate::acp::channel_tools::ChannelToolService,
    authority: &super::authority::SessionContext,
    request: ChannelRequest,
) -> Result<CallToolResult, ErrorData> {
    let task = task_context::validate(authority.connection_id(), &request.task.id)
        .map_err(|_| unavailable())?;
    crate::chat_channel::task_progress::validate_execution(&task)
        .await
        .map_err(|_| unavailable())?;
    let caller = crate::acp::channel_tools::ChannelCaller {
        agent_type: authority.agent_type().to_string(),
        session_ref: task.id.clone(),
        caller_scope: format!("channel-task:{}", task.id),
        working_dir: authority.cwd().to_path_buf(),
    };
    let result = service
        .execute_current(caller, task, (&request.operation, request.arguments))
        .await;
    Ok(
        if result.get("error").is_some()
            || matches!(result["status"].as_str(), Some("failed" | "unknown"))
        {
            CallToolResult::structured_error(result)
        } else {
            CallToolResult::structured(result)
        },
    )
}

fn unavailable() -> ErrorData {
    ErrorData::invalid_request(
        "Current-channel capability is unavailable or the task context changed",
        Some(json!({"code":"channel_context_stale","execution_status":"not_started"})),
    )
}

fn operation_matches(id: &str, query: &str) -> bool {
    let terms: &[&str] = if id.contains(".progress.") {
        &["进度", "状态", "progress", "status"]
    } else if id.contains(".reporting.") {
        &["汇报", "report", "周期"]
    } else if id.contains(".milestone.") {
        &["进展", "里程碑", "milestone"]
    } else if id.contains(".send.") {
        &["发送", "发给", "send", "deliver"]
    } else if id.contains(".card.") {
        &["卡片", "card"]
    } else if id.contains(".stream.") {
        &["流式", "stream"]
    } else if id.contains(".business.") {
        &[
            "文档", "表格", "日程", "document", "sheet", "calendar", "business",
        ]
    } else {
        &[]
    };
    terms.iter().any(|term| query.contains(term))
}

fn card_entry(task: &TaskContext) -> Value {
    entry(
        task,
        "card",
        (
            "向当前聊天发送带操作的卡片；按钮只能使用本任务的进度、汇报控制动作",
            scoped(json!({
                "request_id":{"type":"string","minLength":1,"maxLength":128},
                "title":{"type":"string","minLength":1,"maxLength":80},
                "body":{"type":"string","minLength":1,"maxLength":2000}
            })),
        ),
    )
}
