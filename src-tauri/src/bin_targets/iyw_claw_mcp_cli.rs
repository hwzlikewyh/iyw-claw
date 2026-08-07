use std::process::ExitCode;

use iyw_claw_lib::acp::automation_tools::{ScheduledTaskOperation, ScheduledTaskRequest};
use iyw_claw_lib::acp::delegation::transport::client_automation_round_trip;
use serde_json::{json, Value};
use tokio::io::AsyncReadExt;

struct ToolArgs {
    operation: ScheduledTaskOperation,
    socket_path: String,
    input: Value,
    agent_type: Option<String>,
}

#[derive(Default)]
struct ToolOptions {
    socket_path: Option<String>,
    input_json: Option<String>,
    from_stdin: bool,
    agent_type: Option<String>,
}

pub async fn run_tool_cli(raw: &[String]) -> ExitCode {
    let args = match parse_tool_args(raw).await {
        Ok(args) => args,
        Err(error) => return print_error(error, 2),
    };
    let request = ScheduledTaskRequest {
        operation: args.operation,
        input: args.input,
        caller_agent_type: args.agent_type,
    };
    match client_automation_round_trip(&args.socket_path, &request).await {
        Ok(response) => {
            println!("{}", response.outcome);
            if response.outcome.get("error").is_some() {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(error) => print_error(format!("scheduled task service unavailable: {error}"), 1),
    }
}

async fn parse_tool_args(raw: &[String]) -> Result<ToolArgs, String> {
    if raw.first().map(String::as_str) != Some("scheduled-task") {
        return Err("expected `tool scheduled-task <list|create|update|delete>`".to_string());
    }
    let operation = parse_operation(raw.get(1).map(String::as_str))?;
    let options = parse_options(&raw[2..])?;
    let input = read_input(&options).await?;
    let socket_path = options
        .socket_path
        .or_else(|| std::env::var("IYW_CLAW_TOOL_SOCKET").ok())
        .filter(|value| !value.trim().is_empty())
        .ok_or("missing --socket-path and IYW_CLAW_TOOL_SOCKET".to_string())?;
    Ok(ToolArgs {
        operation,
        socket_path,
        input,
        agent_type: options
            .agent_type
            .or_else(|| std::env::var("IYW_CLAW_AGENT_TYPE").ok()),
    })
}

fn parse_operation(raw: Option<&str>) -> Result<ScheduledTaskOperation, String> {
    match raw {
        Some("list") => Ok(ScheduledTaskOperation::List),
        Some("create") => Ok(ScheduledTaskOperation::Create),
        Some("update") => Ok(ScheduledTaskOperation::Update),
        Some("delete") => Ok(ScheduledTaskOperation::Delete),
        _ => Err("expected scheduled-task operation: list, create, update, or delete".into()),
    }
}

fn parse_options(raw: &[String]) -> Result<ToolOptions, String> {
    let mut options = ToolOptions::default();
    let mut index = 0;
    while index < raw.len() {
        match raw[index].as_str() {
            "--socket-path" => options.socket_path = Some(next_value(raw, &mut index)?),
            "--input" => options.input_json = Some(next_value(raw, &mut index)?),
            "--agent-type" => options.agent_type = Some(next_value(raw, &mut index)?),
            "--stdin" => options.from_stdin = true,
            other => return Err(format!("unknown tool arg: {other}")),
        }
        index += 1;
    }
    if options.from_stdin && options.input_json.is_some() {
        return Err("use only one of --input or --stdin".to_string());
    }
    Ok(options)
}

async fn read_input(options: &ToolOptions) -> Result<Value, String> {
    let raw = if options.from_stdin {
        let mut bytes = Vec::new();
        tokio::io::stdin()
            .read_to_end(&mut bytes)
            .await
            .map_err(|error| format!("read stdin: {error}"))?;
        String::from_utf8(bytes).map_err(|error| format!("stdin must be UTF-8: {error}"))?
    } else {
        options.input_json.clone().unwrap_or_else(|| "{}".into())
    };
    serde_json::from_str(&raw).map_err(|error| format!("invalid JSON input: {error}"))
}

fn next_value(raw: &[String], index: &mut usize) -> Result<String, String> {
    let flag = raw[*index].clone();
    *index += 1;
    raw.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn print_error(error: String, code: u8) -> ExitCode {
    println!("{}", json!({ "error": error }));
    ExitCode::from(code)
}
