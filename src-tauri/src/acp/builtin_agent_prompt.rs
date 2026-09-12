use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::acp::agent_storage::AgentStoragePaths;
use crate::acp::error::AcpError;
use crate::models::agent::AgentType;

const TOOL_NAMES: [&str; 5] = ["uv", "uvx", "node", "npm", "git"];
const COMMON_PROMPT: &str = include_str!("../../resources/agent-prompt.md");

const NO_HOST_MCP: &str = r#"## Agent capability boundary

This Agent's ACP adapter is not connected to iyw-claw's main-process built-in MCP server. The built-in capability gateway and host capabilities reachable only through it are unavailable on this route. Use only similarly named tools that are independently advertised in the actual tool list and follow their current schemas. Do not claim, reconstruct, or namespace-guess a missing gateway or its capabilities."#;

#[derive(Debug, Clone)]
pub struct RenderedBuiltinPrompt {
    pub text: Arc<str>,
    pub hash: String,
}

pub fn render(
    agent_type: AgentType,
    paths: Option<&AgentStoragePaths>,
    response_style: Option<&str>,
) -> Result<RenderedBuiltinPrompt, AcpError> {
    let tools = discover_tools(paths)
        .into_iter()
        .map(|(name, path)| match path {
            Some(path) => format!("- {name}: {}", path.display()),
            None => format!("- {name}: unavailable"),
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut sections = vec![COMMON_PROMPT
        .replace("{tools}", &tools)
        .replace("{skill_runtime}", &crate::shared_runtime::prompt_context())];
    #[cfg(windows)]
    sections.push(super::windows_shell::prompt());
    if !native_response_style_is_sufficient(agent_type, response_style) {
        if let Some(style) = response_style_instruction(response_style) {
            sections.push(style.to_string());
        }
    }
    if matches!(agent_type, AgentType::OpenClaw | AgentType::Pi) {
        sections.push(NO_HOST_MCP.to_string());
    }
    let text = sections.join("\n\n");
    if text.trim().is_empty() {
        return Err(AcpError::BuiltinPromptRender(
            "rendered prompt was empty".to_string(),
        ));
    }
    let hash = format!("{:x}", Sha256::digest(text.as_bytes()));
    Ok(RenderedBuiltinPrompt {
        text: Arc::from(text),
        hash,
    })
}

fn response_style_instruction(style: Option<&str>) -> Option<&'static str> {
    match style {
        Some("standard") => Some(
            "## Response style override\n\nUse a balanced response: lead with the outcome, then include the necessary explanation and verification details. Avoid repeating the request or narrating routine implementation steps.",
        ),
        Some("detailed") => Some(
            "## Response style override\n\nUse a detailed response when it helps the user make a decision or understand the work: lead with the outcome, then include relevant background, alternatives, steps, verification, and material risks. Avoid filler and repetition.",
        ),
        Some("concise") | None => None,
        Some(_) => None,
    }
}

/// Map the shared settings value to Codex's native personality option.
pub fn codex_personality(style: Option<&str>) -> Option<&'static str> {
    match style {
        Some("concise") => Some("pragmatic"),
        Some("standard" | "detailed") => Some("none"),
        _ => None,
    }
}

/// Map the shared settings value to Claude Code's native output style.
pub fn claude_output_style(style: Option<&str>) -> Option<&'static str> {
    match style {
        Some("concise") => Some("Concise"),
        Some("standard") => Some("Default"),
        Some("detailed") => Some("Explanatory"),
        _ => None,
    }
}

fn native_response_style_is_sufficient(agent_type: AgentType, style: Option<&str>) -> bool {
    match agent_type {
        AgentType::ClaudeCode => claude_output_style(style).is_some(),
        AgentType::Codex => style == Some("concise"),
        _ => false,
    }
}

pub(crate) fn discover_tools(
    paths: Option<&AgentStoragePaths>,
) -> Vec<(&'static str, Option<PathBuf>)> {
    TOOL_NAMES
        .into_iter()
        .map(|name| (name, resolve_tool(paths, name)))
        .collect()
}

fn resolve_tool(paths: Option<&AgentStoragePaths>, name: &str) -> Option<PathBuf> {
    if let Some(managed) = crate::acp::version_center::managed_tool_executable(name) {
        return Some(managed);
    }
    if matches!(name, "uv" | "uvx") {
        if let Some(tool) =
            paths.and_then(|paths| crate::acp::binary_cache::find_cached_uv_tool(paths, name))
        {
            return Some(tool);
        }
    }
    which::which(name).ok()
}

pub struct EnvironmentRequest<'a> {
    pub agent_type: AgentType,
    pub environment: &'a mut BTreeMap<String, String>,
    pub prompt: &'a str,
    pub response_style: Option<&'a str>,
    pub opencode_instruction: Option<&'a Path>,
}

pub fn apply_environment(request: EnvironmentRequest<'_>) -> Result<(), AcpError> {
    match request.agent_type {
        AgentType::Codex => merge_codex_instructions(
            request.environment,
            request.prompt,
            codex_personality(request.response_style),
        ),
        AgentType::OpenCode => {
            merge_opencode_instructions(request.environment, request.opencode_instruction)
        }
        AgentType::Hermes => {
            append_environment(
                request.environment,
                "HERMES_ENVIRONMENT_HINT",
                request.prompt,
            );
            Ok(())
        }
        _ => Ok(()),
    }
}

fn merge_codex_instructions(
    environment: &mut BTreeMap<String, String>,
    prompt: &str,
    personality: Option<&str>,
) -> Result<(), AcpError> {
    const ENV_KEY: &str = "CODEX_CONFIG";
    const FIELD: &str = "developer_instructions";
    let mut object = parse_json_object(environment.get(ENV_KEY), ENV_KEY)?;
    let previous = match object.remove(FIELD) {
        Some(Value::String(value)) => value,
        Some(Value::Null) | None => String::new(),
        Some(_) => {
            return Err(injection_error(format!(
                "{ENV_KEY}.{FIELD} must be a string"
            )))
        }
    };
    object.insert(
        FIELD.to_string(),
        Value::String(append_text(&previous, prompt)),
    );
    if let Some(personality) = personality {
        object.insert(
            "personality".to_string(),
            Value::String(personality.to_string()),
        );
    }
    serialize_environment(environment, ENV_KEY, object)
}

fn merge_opencode_instructions(
    environment: &mut BTreeMap<String, String>,
    instruction: Option<&Path>,
) -> Result<(), AcpError> {
    let path = instruction.ok_or_else(|| injection_error("OpenCode prompt file is missing"))?;
    let mut object = parse_json_object(
        environment.get("OPENCODE_CONFIG_CONTENT"),
        "OPENCODE_CONFIG_CONTENT",
    )?;
    let mut values = match object.remove("instructions") {
        Some(Value::Array(values)) => values,
        Some(Value::String(value)) => vec![Value::String(value)],
        Some(Value::Null) | None => Vec::new(),
        Some(_) => {
            return Err(injection_error(
                "OPENCODE_CONFIG_CONTENT.instructions must be a string or array",
            ))
        }
    };
    let path = Value::String(path.to_string_lossy().into_owned());
    if !values.contains(&path) {
        values.push(path);
    }
    object.insert("instructions".to_string(), Value::Array(values));
    serialize_environment(environment, "OPENCODE_CONFIG_CONTENT", object)
}

fn parse_json_object(raw: Option<&String>, key: &str) -> Result<Map<String, Value>, AcpError> {
    let Some(raw) = raw.filter(|value| !value.trim().is_empty()) else {
        return Ok(Map::new());
    };
    serde_json::from_str::<Value>(raw)
        .map_err(|error| injection_error(format!("invalid {key}: {error}")))?
        .as_object()
        .cloned()
        .ok_or_else(|| injection_error(format!("{key} must be a JSON object")))
}

fn serialize_environment(
    environment: &mut BTreeMap<String, String>,
    key: &str,
    object: Map<String, Value>,
) -> Result<(), AcpError> {
    let value = serde_json::to_string(&Value::Object(object))
        .map_err(|error| injection_error(format!("failed to serialize {key}: {error}")))?;
    environment.insert(key.to_string(), value);
    Ok(())
}

fn append_environment(environment: &mut BTreeMap<String, String>, key: &str, prompt: &str) {
    let previous = environment.get(key).map(String::as_str).unwrap_or_default();
    environment.insert(key.to_string(), append_text(previous, prompt));
}

fn append_text(previous: &str, prompt: &str) -> String {
    match previous.trim() {
        "" => prompt.to_string(),
        _ => format!("{}\n\n{prompt}", previous.trim_end()),
    }
}

pub fn append_launch_args(agent_type: AgentType, args: &mut Vec<String>, prompt: &str) {
    if agent_type == AgentType::CodeBuddy {
        args.extend(["--append-system-prompt".to_string(), prompt.to_string()]);
    }
}

pub fn session_meta(
    agent_type: AgentType,
    prompt: &str,
    openclaw_session_key: Option<&str>,
    response_style: Option<&str>,
) -> Option<Map<String, Value>> {
    let mut meta = Map::new();
    if agent_type == AgentType::ClaudeCode {
        let mut claude_code = serde_json::json!({"emitRawSDKMessages": true});
        if let Some(output_style) = claude_output_style(response_style) {
            claude_code["options"] = serde_json::json!({
                "settings": {"outputStyle": output_style}
            });
        }
        meta.insert("claudeCode".to_string(), claude_code);
        meta.insert(
            "systemPrompt".to_string(),
            serde_json::json!({"append": prompt}),
        );
    }
    if agent_type == AgentType::OpenClaw {
        if let Some(key) = openclaw_session_key {
            meta.insert("sessionKey".to_string(), Value::String(key.to_string()));
        }
    }
    (!meta.is_empty()).then_some(meta)
}

fn injection_error(message: impl Into<String>) -> AcpError {
    AcpError::BuiltinPromptInjection(message.into())
}
