use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sha2::{Digest, Sha256};

use super::{
    build_agent, internal_codex_worker_requested, runtime_host_key, runtime_host_process_cwd,
    shared_runtime_host_enabled, stderr_tail_for_runtime_host, AcpError, AgentLaunchSpec,
    AgentStoragePaths, AgentType, InternalWorkerLaunch,
};
use crate::acp::runtime_host::RuntimeHostRegistry;

pub(crate) struct RuntimePrewarmTarget {
    pub(crate) cwd: PathBuf,
    pub(crate) session_id: Option<String>,
}

pub(crate) struct RuntimePrewarmRequest {
    pub(crate) agent_type: AgentType,
    pub(crate) environment: BTreeMap<String, String>,
    pub(crate) target: RuntimePrewarmTarget,
}

pub(super) struct OwnedRuntimeScope<'a> {
    pub(super) cwd: &'a Path,
    pub(super) session_id: Option<&'a str>,
    pub(super) environment: &'a BTreeMap<String, String>,
}

pub(super) fn owned_fingerprint(base: String, scope: OwnedRuntimeScope<'_>) -> String {
    let mut digest = Sha256::new();
    for value in [
        base.as_bytes(),
        scope.cwd.as_os_str().as_encoded_bytes(),
        scope.session_id.unwrap_or("").as_bytes(),
    ] {
        digest.update(value.len().to_le_bytes());
        digest.update(value);
    }
    // staleness 指纹有意忽略并发上限等字段；预热接管必须匹配完整启动环境。
    for (key, value) in scope.environment {
        for part in [key, value] {
            digest.update(part.len().to_le_bytes());
            digest.update(part.as_bytes());
        }
    }
    format!("{:x}:owned", digest.finalize())
}

pub(crate) async fn prewarm_agent_runtime(
    request: RuntimePrewarmRequest,
    runtime_hosts: Arc<RuntimeHostRegistry>,
) -> Result<bool, AcpError> {
    if !matches!(request.agent_type, AgentType::Codex | AgentType::ClaudeCode)
        || !request.target.cwd.is_dir()
    {
        return Ok(false);
    }
    let storage = AgentStoragePaths::active()
        .ok_or_else(|| AcpError::protocol("Agent storage unavailable"))?;
    let prepared = prepare_prompt(&request, &storage).await?;
    let launch = prepare_host(&request, &prepared, &storage).await?;
    runtime_hosts
        .prewarm(launch.key, launch.agent, launch.stderr_tail)
        .await
}

async fn prepare_prompt(
    request: &RuntimePrewarmRequest,
    storage: &AgentStoragePaths,
) -> Result<crate::acp::builtin_prompt_injection::PreparedBuiltinPrompt, AcpError> {
    let agent_type = request.agent_type;
    let overlay = if request.target.session_id.is_some() {
        crate::acp::provider_overlay::enforce_resumed_active_provider_overlay(agent_type)
    } else {
        crate::acp::provider_overlay::enforce_active_provider_overlay(agent_type)
    };
    overlay.map_err(AcpError::protocol)?;
    crate::commands::experts::ensure_builtin_gateway_skill_ready(agent_type)
        .await
        .map_err(|error| {
            AcpError::protocol(format!("Capability gateway Skill is not ready: {error}"))
        })?;
    crate::acp::builtin_prompt_injection::prepare(
        crate::acp::builtin_prompt_injection::PrepareRequest {
            agent_type,
            connection_id: "runtime-host-prewarm",
            session_id: request.target.session_id.as_deref(),
            environment: &request.environment,
            storage,
            response_style: Some("concise"),
            is_delegation_child: false,
        },
    )
    .await
}

struct PreparedHost {
    key: crate::acp::runtime_host::RuntimeHostKey,
    agent: sacp_tokio::AcpAgent,
    stderr_tail: Arc<crate::acp::stderr_tail::StderrTail>,
}

async fn prepare_host(
    request: &RuntimePrewarmRequest,
    prepared: &crate::acp::builtin_prompt_injection::PreparedBuiltinPrompt,
    storage: &AgentStoragePaths,
) -> Result<PreparedHost, AcpError> {
    let agent_type = request.agent_type;
    let dedicated = internal_codex_worker_requested(agent_type, &prepared.environment);
    let shared = shared_runtime_host_enabled(agent_type) && !dedicated;
    let cwd = if dedicated {
        request.target.cwd.as_path()
    } else {
        runtime_host_process_cwd(agent_type, storage, &request.target.cwd)
    };
    let base = crate::commands::acp::fingerprint_config(agent_type, &prepared.environment);
    let fingerprint = if shared {
        base
    } else {
        owned_fingerprint(
            base,
            OwnedRuntimeScope {
                cwd,
                session_id: request.target.session_id.as_deref(),
                environment: &prepared.environment,
            },
        )
    };
    let stderr_tail = stderr_tail_for_runtime_host(agent_type, shared);
    let agent = build_agent(AgentLaunchSpec {
        agent_type,
        internal_worker: dedicated.then_some(InternalWorkerLaunch {
            expected_session_id: request.target.session_id.as_deref(),
            runtime_fingerprint: &fingerprint,
        }),
        runtime_env: &prepared.environment,
        cwd,
        builtin_prompt: &prepared.prompt.text,
        stderr_tail: &stderr_tail,
    })
    .await?;
    let key = runtime_host_key(agent_type, fingerprint, dedicated).await?;
    Ok(PreparedHost {
        key,
        agent,
        stderr_tail,
    })
}
