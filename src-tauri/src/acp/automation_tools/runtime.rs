use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use crate::models::agent::AgentType;

const TOOL_BIN_ENV: &str = "IYW_CLAW_TOOL_BIN";
const TOOL_SOCKET_ENV: &str = "IYW_CLAW_TOOL_SOCKET";
const AGENT_TYPE_ENV: &str = "IYW_CLAW_AGENT_TYPE";

struct ScheduledTaskRuntime {
    socket_path: PathBuf,
    tool_bin: Option<PathBuf>,
}

static RUNTIME: OnceLock<ScheduledTaskRuntime> = OnceLock::new();

pub fn install_scheduled_task_runtime(socket_path: PathBuf, tool_bin: Option<PathBuf>) {
    let available = tool_bin.is_some();
    if RUNTIME
        .set(ScheduledTaskRuntime {
            socket_path,
            tool_bin,
        })
        .is_err()
    {
        tracing::debug!("[automation-tool] runtime already installed");
        return;
    }
    tracing::info!(available, "[automation-tool] CLI runtime installed");
}

pub fn inject_scheduled_task_env(
    agent_type: AgentType,
    environment: &mut BTreeMap<String, String>,
) {
    let Some(runtime) = RUNTIME.get() else {
        return;
    };
    let Some(tool_bin) = runtime.tool_bin.as_ref() else {
        return;
    };
    environment.insert(
        TOOL_BIN_ENV.to_string(),
        tool_bin.to_string_lossy().into_owned(),
    );
    environment.insert(
        TOOL_SOCKET_ENV.to_string(),
        runtime.socket_path.to_string_lossy().into_owned(),
    );
    if let Some(agent_type) = agent_type_value(agent_type) {
        environment.insert(AGENT_TYPE_ENV.to_string(), agent_type);
    }
}

pub fn scheduled_task_cli_path() -> Option<PathBuf> {
    RUNTIME
        .get()
        .and_then(|runtime| runtime.tool_bin.as_ref())
        .cloned()
}

fn agent_type_value(agent_type: AgentType) -> Option<String> {
    serde_json::to_value(agent_type)
        .ok()?
        .as_str()
        .map(str::to_string)
}
