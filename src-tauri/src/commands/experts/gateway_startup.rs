use super::{
    agent_link_path, classify_link, expert_central_path, expert_status_from_link_change,
    link_central_skill_locked, managed_copy_is_owned, AgentType, ExpertInstallStatus,
    ExpertLinkState, ExpertsError, CAPABILITY_GATEWAY_EXPERT_ID,
};

pub(super) fn publish(agent: AgentType) -> Result<ExpertInstallStatus, ExpertsError> {
    match link_central_skill_locked(CAPABILITY_GATEWAY_EXPERT_ID, agent) {
        Ok(status) => Ok(status),
        Err(error) if supports_direct_gateway(agent) && directory_links_failed(&error) => {
            // 只降级目录发布方式；中央包仍须完整安装、校验并可读。
            let prompt = direct_gateway_skill_prompt(agent)?;
            if prompt.is_none() {
                return Err(error);
            }
            let path = agent_link_path(agent, CAPABILITY_GATEWAY_EXPERT_ID)?;
            tracing::warn!(
                agent = %agent,
                error = %error,
                "[skills] directory links unavailable; publishing gateway through built-in instructions"
            );
            Ok(expert_status_from_link_change(
                CAPABILITY_GATEWAY_EXPERT_ID,
                agent,
                (path, false),
            ))
        }
        Err(error) => Err(error),
    }
}

fn supports_direct_gateway(agent: AgentType) -> bool {
    cfg!(windows) && matches!(agent, AgentType::Codex | AgentType::ClaudeCode)
}

fn directory_links_failed(error: &ExpertsError) -> bool {
    matches!(error, ExpertsError::Io(message)
        if message.contains("failed to create Skill directory link '")
            && message.contains("junction failed:")
            && message.contains("directory symbolic link failed:"))
}

/// 复用星河与远山已有的内置指令注入，不复制技能，也不伪造原生链接状态。
pub(crate) fn direct_gateway_skill_prompt(
    agent: AgentType,
) -> Result<Option<String>, ExpertsError> {
    if !supports_direct_gateway(agent) || !needs_direct_gateway(agent)? {
        return Ok(None);
    }
    let skill = expert_central_path(CAPABILITY_GATEWAY_EXPERT_ID).join("SKILL.md");
    let content = std::fs::read_to_string(&skill).map_err(|error| {
        ExpertsError::Io(format!(
            "cannot read central capability gateway Skill: {error}"
        ))
    })?;
    if !skill.is_absolute() || content.trim().is_empty() {
        return Err(ExpertsError::CentralUnavailable(
            "central capability gateway Skill must be nonempty and have an absolute path".into(),
        ));
    }
    let path =
        serde_json::to_string(&skill).map_err(|error| ExpertsError::Metadata(error.to_string()))?;
    Ok(Some(format!(
        "## Available built-in capability gateway Skill\n\n\
         The application enables iyw-capability-gateway directly from its central store. \
         Its SKILL.md path (JSON string) is {path}. This Skill remains available even if \
         the native Skill list omits it. For IYW business tools, social research, images, \
         video, uploads, memory, host actions, sessions, artifacts, browser, audio, \
         channels, automation, interaction or delegation, read this SKILL.md before \
         using the capability and follow its relevant references. Resolve relative \
         references against the directory containing this SKILL.md. Reuse instructions \
         already read in the conversation; read only relevant references. Keep this \
         central directory as the single source; do not create per-Agent Skill copies."
    )))
}

fn needs_direct_gateway(agent: AgentType) -> Result<bool, ExpertsError> {
    let central = expert_central_path(CAPABILITY_GATEWAY_EXPERT_ID);
    let link = agent_link_path(agent, CAPABILITY_GATEWAY_EXPERT_ID)?;
    match std::fs::symlink_metadata(&link) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(error.into()),
        Ok(_) if classify_link(&link, &central) == ExpertLinkState::LinkedToIywClaw
            && !managed_copy_is_owned(&central, &link) => Ok(false),
        _ => Err(ExpertsError::CentralUnavailable(
            "capability gateway Skill path conflicts with the central source; existing entry preserved"
                .into(),
        )),
    }
}
