use std::collections::BTreeMap;

use crate::acp::agent_storage::AgentStoragePaths;
use crate::acp::builtin_agent_prompt::{self, RenderedBuiltinPrompt};
use crate::acp::builtin_prompt_bridge::PreparedPromptBridges;
use crate::acp::builtin_prompt_openclaw::{self, OpenClawPromptRoute};
use crate::acp::error::AcpError;
use crate::models::agent::AgentType;

pub struct PrepareRequest<'a> {
    pub agent_type: AgentType,
    pub connection_id: &'a str,
    pub session_id: Option<&'a str>,
    pub environment: &'a BTreeMap<String, String>,
    pub storage: &'a AgentStoragePaths,
    pub response_style: Option<&'a str>,
    pub is_delegation_child: bool,
}

pub struct PreparedBuiltinPrompt {
    pub environment: BTreeMap<String, String>,
    pub prompt: RenderedBuiltinPrompt,
    pub bridges: PreparedPromptBridges,
    pub openclaw: OpenClawPromptRoute,
}

pub async fn prepare(request: PrepareRequest<'_>) -> Result<PreparedBuiltinPrompt, AcpError> {
    let response_style = if request.is_delegation_child {
        None
    } else {
        request.response_style
    };
    let prompt =
        builtin_agent_prompt::render(request.agent_type, Some(request.storage), response_style)?;
    let bridges =
        PreparedPromptBridges::prepare(crate::acp::builtin_prompt_bridge::PrepareRequest {
            agent_type: request.agent_type,
            connection_id: request.connection_id,
            prompt: &prompt,
            storage: request.storage,
        })?;
    let openclaw = if request.agent_type == AgentType::OpenClaw {
        builtin_prompt_openclaw::prepare(builtin_prompt_openclaw::PrepareRequest {
            storage: request.storage,
            environment: request.environment,
            prompt: &prompt,
            session_id: request.session_id,
        })
        .await?
    } else {
        OpenClawPromptRoute::default()
    };
    let mut environment = request.environment.clone();
    if let Some(session_key) = openclaw.session_key.as_ref() {
        environment.insert("OPENCLAW_SESSION_KEY".to_string(), session_key.clone());
    }
    builtin_agent_prompt::apply_environment(builtin_agent_prompt::EnvironmentRequest {
        agent_type: request.agent_type,
        environment: &mut environment,
        prompt: &prompt.text,
        response_style,
        opencode_instruction: bridges.opencode_instruction.as_deref(),
    })?;
    Ok(PreparedBuiltinPrompt {
        environment,
        prompt,
        bridges,
        openclaw,
    })
}
