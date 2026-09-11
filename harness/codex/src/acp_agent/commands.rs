use sacp::{Client, ConnectionTo};
use serde_json::{json, Value};

use super::{send_update, to_sacp_error};
use crate::{UpstreamClient, UpstreamError};

pub(super) struct CommandContext<'a> {
    pub upstream: &'a UpstreamClient,
    pub cx: &'a ConnectionTo<Client>,
    pub session: Option<&'a str>,
    pub settings: &'a mut super::settings_mapping::SessionSettings,
    pub cwd: &'a std::path::Path,
}

pub(super) async fn handle(
    params: &Value,
    context: CommandContext<'_>,
) -> Result<bool, UpstreamError> {
    let Some(command) = command(params) else {
        return Ok(false);
    };
    let id = params["sessionId"]
        .as_str()
        .filter(|id| context.session == Some(*id))
        .ok_or_else(|| {
            UpstreamError::InvalidRequest(
                "command session does not match the owning session".into(),
            )
        })?;
    if context.upstream.active_turn_for(id).await.is_some() {
        return Err(UpstreamError::InvalidRequest(
            "cannot run a slash command during an active turn".into(),
        ));
    }
    let text = match command.0 {
        "plan" => {
            let params =
                json!({ "sessionId": id, "configId": "collaboration_mode", "value": "plan" });
            let (request, change) = super::settings_mapping::request(
                "session/set_config_option",
                &params,
                context.settings,
            )?;
            context
                .upstream
                .request_json_for_thread(id, request)
                .await?;
            context.settings.apply(change);
            send_update(context.cx, &Some(id.to_string()), "config_option_update", json!({ "configOptions": super::settings_mapping::config_options(context.settings) }))
                .map_err(|error| UpstreamError::Io(error.to_string()))?;
            "已切换到计划模式。".into()
        }
        "skills" => {
            publish(context.upstream, context.cx, (id, context.cwd))
                .await
                .map_err(|error| UpstreamError::Io(error.to_string()))?;
            "已更新当前可用技能，可在命令列表中选择。".into()
        }
        _ => execute(context.upstream, id, command).await?,
    };
    send_update(
        context.cx,
        &Some(id.to_string()),
        "agent_message_chunk",
        json!({ "content": { "type": "text", "text": text } }),
    )
    .map_err(|error| UpstreamError::Io(error.to_string()))?;
    Ok(true)
}

pub(super) async fn publish(
    upstream: &UpstreamClient,
    cx: &ConnectionTo<Client>,
    session: (&str, &std::path::Path),
) -> Result<(), sacp::Error> {
    let (id, cwd) = session;
    let response = upstream
        .request_global_json(json!({ "method": "skills/list", "params": { "cwds": [cwd] } }))
        .await
        .map_err(to_sacp_error)?;
    let mut commands = vec![
        json!({ "name": "plan", "description": "切换计划模式", "input": null, "_meta": { "commandAction": {
            "kind": "setConfigOption", "configId": "collaboration_mode", "value": "plan", "resetValue": "default", "presentation": "state",
        } } }),
        json!({ "name": "compact", "description": "压缩当前会话上下文", "input": null }),
        json!({ "name": "mcp", "description": "查看当前 MCP 服务器", "input": null }),
        json!({ "name": "skills", "description": "查看当前可用技能", "input": null }),
        json!({ "name": "goal", "description": "设置持续执行目标", "input": { "hint": "目标内容；clear 清除目标" } }),
    ];
    for entry in response["data"].as_array().into_iter().flatten() {
        for skill in entry["skills"].as_array().into_iter().flatten() {
            let Some(name) = skill["name"].as_str() else {
                continue;
            };
            if skill["enabled"] == false {
                continue;
            }
            let name = format!("${name}");
            if commands.iter().any(|command| command["name"] == name) {
                continue;
            }
            let description = skill
                .get("shortDescription")
                .filter(|value| value.is_string())
                .or_else(|| skill.get("description"))
                .and_then(Value::as_str)
                .unwrap_or(&name);
            commands.push(json!({ "name": name, "description": description, "input": null }));
        }
    }
    send_update(
        cx,
        &Some(id.to_string()),
        "available_commands_update",
        json!({ "availableCommands": commands }),
    )
}

pub(super) fn command(params: &Value) -> Option<(&str, &str)> {
    let prompt = params["prompt"].as_array()?;
    let prompt = if prompt.first().and_then(|block| block["text"].as_str()).is_some_and(|text| {
        text.starts_with("<!-- IYW_CLAW_USER_CONTEXT_V1_START -->")
            && text.trim_end().ends_with("<!-- IYW_CLAW_USER_CONTEXT_V1_END -->")
    }) { &prompt[1..] } else { prompt.as_slice() };
    if prompt.len() != 1 || prompt[0]["type"] != "text" {
        return None;
    }
    let text = prompt[0]["text"].as_str()?.trim().strip_prefix('/')?;
    let (command, args) = text.split_once(char::is_whitespace).unwrap_or((text, ""));
    matches!(command, "plan" | "compact" | "mcp" | "skills" | "goal")
        .then_some((command, args.trim()))
}

pub(super) async fn execute(
    upstream: &UpstreamClient,
    thread: &str,
    command: (&str, &str),
) -> Result<String, UpstreamError> {
    let (name, args) = command;
    match name {
        "compact" => {
            upstream
                .request_json_for_thread(
                    thread,
                    json!({ "method": "thread/compact/start", "params": { "threadId": thread } }),
                )
                .await?;
            Ok("已开始压缩会话上下文。".into())
        }
        "goal" => {
            let method = if args == "clear" {
                "thread/goal/clear"
            } else if args.is_empty() || args == "status" {
                "thread/goal/get"
            } else {
                "thread/goal/set"
            };
            let mut params = json!({ "threadId": thread });
            if method == "thread/goal/set" {
                match args {
                    "pause" => params["status"] = json!("paused"),
                    "resume" => params["status"] = json!("active"),
                    "set" | "edit" => return Err(UpstreamError::InvalidRequest("Goal objective is required".into())),
                    objective => {
                        let objective = objective.strip_prefix("set ").or_else(|| objective.strip_prefix("edit ")).unwrap_or(objective).trim();
                        if objective.is_empty() { return Err(UpstreamError::InvalidRequest("Goal objective is required".into())); }
                        params["objective"] = json!(objective);
                        params["status"] = json!("active");
                    }
                }
            }
            let result = upstream
                .request_json_for_thread(thread, json!({ "method": method, "params": params }))
                .await?;
            let goal = result.get("goal").unwrap_or(&result);
            Ok(if method == "thread/goal/clear" {
                "已清除当前目标。".into()
            } else if let Some(objective) = goal.get("objective").and_then(Value::as_str) {
                objective.to_string()
            } else {
                "当前没有持续执行目标。".into()
            })
        }
        "mcp" => {
            let names = mcp_names(upstream, thread).await?;
            Ok(if names.is_empty() {
                "当前没有 MCP 服务器。".into()
            } else {
                names.join("\n")
            })
        }
        _ => Err(UpstreamError::InvalidRequest(
            "command needs session configuration".into(),
        )),
    }
}

async fn mcp_names(upstream: &UpstreamClient, thread: &str) -> Result<Vec<String>, UpstreamError> {
    let mut cursor = Value::Null;
    let mut visited = std::collections::BTreeSet::new();
    let mut names = Vec::new();
    loop {
        let page = upstream.request_global_json(json!({
            "method": "mcpServerStatus/list", "params": { "threadId": thread, "cursor": cursor },
        })).await?;
        let servers = page["data"]
            .as_array()
            .ok_or_else(|| UpstreamError::InvalidResponse("MCP status page has no data".into()))?;
        names.extend(
            servers
                .iter()
                .filter_map(|server| server["name"].as_str().map(str::to_string)),
        );
        let Some(next) = page["nextCursor"].as_str() else {
            return Ok(names);
        };
        if !visited.insert(next.to_string()) {
            return Err(UpstreamError::InvalidResponse(
                "MCP status repeated a cursor".into(),
            ));
        }
        cursor = json!(next);
    }
}
