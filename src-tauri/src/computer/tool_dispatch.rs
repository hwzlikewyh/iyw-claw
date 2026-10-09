use serde_json::Value;

use super::{tool_arguments as args, tool_render as render};
use crate::commands::computer::ComputerService;

pub(crate) struct ComputerCall<'a> {
    pub tool: &'a str,
    pub arguments: &'a Value,
}

pub(crate) async fn execute(
    service: &ComputerService,
    call: ComputerCall<'_>,
) -> Result<Value, String> {
    CALL_STOP
        .scope(service.operation_epoch(), execute_inner(service, call))
        .await
}

tokio::task_local! { pub(crate) static CALL_STOP: u64; }
tokio::task_local! { pub(crate) static CALL_ACTOR: ComputerActor; }

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ComputerActor {
    pub connection_id: String,
    pub agent: String,
}

async fn execute_inner(service: &ComputerService, call: ComputerCall<'_>) -> Result<Value, String> {
    let input = call.arguments;
    match call.tool {
        "computer_list_apps" => Ok(render::render_computer_apps_result(&value(
            service.agent_list_apps().await,
        )?)),
        "computer_list_windows" => {
            let pid = args::computer_optional_u32(input, call.tool, "pid")?;
            Ok(render::render_computer_windows_result(&value(
                service.agent_list_windows(pid).await,
            )?))
        }
        "computer_screenshot" => {
            let (target, max) = args::computer_capture_request(input)?;
            Ok(render::render_computer_capture_result(&value(
                service.agent_capture(&target, max).await,
            )?))
        }
        "computer_snapshot" => {
            let (target, request) = args::computer_snapshot_request(input)?;
            Ok(render::render_computer_snapshot_result(&value(
                service.agent_snapshot(&target, request).await,
            )?))
        }
        "computer_verify" => {
            let (target, request) = args::computer_verify_request(input)?;
            Ok(render::render_computer_verify_result(&value(
                service.agent_verify(&target, request).await,
            )?))
        }
        "computer_launch_app" => {
            let (name, key) = args::computer_launch_arguments(input)?;
            Ok(render::render_computer_launch_result(&value(
                service.agent_launch_app(name, key).await,
            )?))
        }
        "computer_clipboard_read" | "computer_clipboard_write" => {
            let request = args::computer_clipboard_op(call.tool, input)?;
            Ok(render::render_computer_clipboard_result(&value(
                service.agent_clipboard(request).await,
            )?))
        }
        tool => {
            let (target, request, delivery) = args::computer_act_request(tool, input)?;
            Ok(render::render_computer_act_result(&value(
                service.agent_act(&target, request, delivery).await,
            )?))
        }
    }
}

fn value(outcome: impl serde::Serialize) -> Result<Value, String> {
    serde_json::to_value(outcome).map_err(|error| error.to_string())
}
