use crate::agent::types::ResolvedMultiAgentV2UsageHints;
use crate::config::MultiAgentV2Config;
use crate::context::MultiAgentRoleInstructions;
use crate::session::step_context::StepContext;
use codex_prompts::ResolvedMessage;
use codex_prompts::ResolvedModelMessages;
use codex_prompts::ResolvedMultiAgentMessages;
use codex_protocol::config_types::MultiAgentMode;
use codex_protocol::openai_models::ReasoningEffort;
use codex_protocol::protocol::MultiAgentVersion;
use codex_protocol::protocol::SessionSource;
use codex_protocol::protocol::SubAgentSource;

const DELEGATION_USAGE_HINT: &str = r#"Delegation workflow:
- Give every child a complete task: goal, absolute working paths, exclusive file scope, constraints, acceptance criteria, and expected output. `fork_turns` controls inherited history only; always supply the task in `message`.
- After dispatch, continue useful independent work immediately. Do not wait for a startup acknowledgment. Children may send a brief progress message after a real file read, then continue without waiting for permission from the parent.
- A child must execute its assigned task and return concrete evidence or a precise blocker. A role acknowledgment is not a final result. Review tasks can return findings and file locations without creating files; code tasks must identify actual changes and validation limits.
- If a child ended without doing the task or explicitly lacked its task, inspect existing work before resending the complete task once with `followup_task`. `send_message` does not wake an idle child. Do not repeatedly respawn the same failed task under new names.
- Before taking over active work, interrupt the child, confirm its turn has stopped, and inspect any commands still running and existing edits. An interrupt acknowledgment alone does not prove background commands stopped.
- At integration, validate each result against its acceptance criteria. A completed turn, a wait timeout, or the absence of new files does not establish success or failure. Do not infer rate limiting without an actual error.
- Children perform focused checks permitted by the repository; the parent performs necessary final checks once after integration. Do not duplicate full builds or override repository test restrictions.
- Call `wait_agent` only when progress depends on an unfinished child and no independent work remains. A timeout is not an agent failure.
"#;

pub(super) fn usage_hint_text(step_context: &StepContext) -> Option<MultiAgentRoleInstructions> {
    let turn_context = step_context.turn.as_ref();
    if turn_context.multi_agent_version != MultiAgentVersion::V2 {
        return None;
    }

    let multi_agent_messages =
        ResolvedModelMessages::from_model(&step_context.settings.model_info).multi_agent();
    let snapshot = resolve_usage_hints(
        &turn_context.config.multi_agent_v2,
        multi_agent_messages,
        !turn_context.config.update_plan_enabled && turn_context.config.model_catalog.is_none(),
    );
    match &turn_context.session_source {
        SessionSource::SubAgent(SubAgentSource::ThreadSpawn { .. }) => snapshot.subagent,
        SessionSource::Cli
        | SessionSource::VSCode
        | SessionSource::Exec
        | SessionSource::Mcp
        | SessionSource::Custom(_)
        | SessionSource::Unknown => snapshot.root,
        SessionSource::Internal(_) | SessionSource::SubAgent(_) => None,
    }
}

pub(crate) fn resolve_usage_hints(
    config: &MultiAgentV2Config,
    multi_agent_messages: ResolvedMultiAgentMessages<'_>,
    omit_update_plan_instructions: bool,
) -> ResolvedMultiAgentV2UsageHints {
    let resolve_role = |configured: Option<&str>, message: ResolvedMessage<'_>| {
        // Configured roles take precedence; empty configured or catalog roles suppress fallback.
        if let Some(configured) = configured {
            return (!configured.is_empty())
                .then(|| MultiAgentRoleInstructions::Configured(configured.to_owned()));
        }

        let base = message.text();
        if base.is_empty() {
            return None;
        }
        Some(MultiAgentRoleInstructions::Composed {
            base: format!("{base}\n\n{DELEGATION_USAGE_HINT}"),
            marked: message.catalog_override().is_some(),
            omit_update_plan_instructions,
            max_concurrency: config.max_concurrent_threads_per_session,
            wait_agent_enabled: config.wait_agent_enabled,
            expose_model_overrides: config.expose_spawn_agent_model_overrides,
        })
    };

    ResolvedMultiAgentV2UsageHints {
        root: resolve_role(
            config.root_agent_usage_hint_text.as_deref(),
            multi_agent_messages.root,
        ),
        subagent: resolve_role(
            config.subagent_usage_hint_text.as_deref(),
            multi_agent_messages.subagent,
        ),
    }
}

pub(crate) fn effective_multi_agent_mode(step_context: &StepContext) -> Option<MultiAgentMode> {
    let turn_context = step_context.turn.as_ref();
    let settings = &step_context.settings;
    if turn_context.multi_agent_version != MultiAgentVersion::V2 {
        return None;
    }

    let multi_agent_messages =
        ResolvedModelMessages::from_model(&settings.model_info).multi_agent();
    let hint = turn_context
        .config
        .multi_agent_v2
        .multi_agent_mode_hint_text
        .as_deref()
        .or(multi_agent_messages.hint);
    let multi_agent_mode = match hint {
        Some(text) => MultiAgentMode::Custom(text.to_owned()),
        None => {
            let (message, builtin) =
                if settings.effective_reasoning_effort() == Some(ReasoningEffort::Ultra) {
                    (multi_agent_messages.proactive, MultiAgentMode::Proactive)
                } else {
                    (
                        multi_agent_messages.explicit,
                        MultiAgentMode::ExplicitRequestOnly,
                    )
                };
            match message {
                ResolvedMessage::Catalog(text) => MultiAgentMode::Custom(text.to_owned()),
                ResolvedMessage::Bundled(_) => builtin,
            }
        }
    };

    match &turn_context.session_source {
        SessionSource::SubAgent(SubAgentSource::ThreadSpawn { .. })
        | SessionSource::Cli
        | SessionSource::VSCode
        | SessionSource::Exec
        | SessionSource::Mcp
        | SessionSource::Custom(_)
        | SessionSource::Unknown => Some(multi_agent_mode),
        SessionSource::Internal(_) | SessionSource::SubAgent(_) => None,
    }
}
