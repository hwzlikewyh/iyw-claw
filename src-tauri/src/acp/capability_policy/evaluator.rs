use chrono::{DateTime, Utc};
use serde::Serialize;

use super::capability::Capability;
use super::dto::{AgentCapabilityPolicy, CapabilityPolicySnapshot};
use super::store::{PolicySnapshotSource, PolicySnapshotView};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSubject {
    pub platform_id: String,
    pub is_existing_agent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicySubject {
    Agent(AgentSubject),
    Client,
}

#[derive(Debug, Clone)]
pub struct CapabilityRequest {
    pub subject: PolicySubject,
    pub capability: Capability,
    pub compiled_support: bool,
    pub local_enabled: bool,
    pub runtime_verified: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionSource {
    RemotePolicy,
    TrustedCache,
    LegacyLastTrusted,
    NoTrustedPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DenialCode {
    CompiledSupportDisabled,
    LocalPreferenceDisabled,
    RuntimeUnverified,
    SubjectCapabilityMismatch,
    RemotePolicyMissing,
    RemotePolicyUnknownAgent,
    RemotePolicyExpired,
    RemotePolicyRollback,
    RemotePolicyDenied,
}

impl DenialCode {
    pub fn key(self) -> &'static str {
        match self {
            Self::CompiledSupportDisabled => "compiled_support_disabled",
            Self::LocalPreferenceDisabled => "local_preference_disabled",
            Self::RuntimeUnverified => "runtime_unverified",
            Self::SubjectCapabilityMismatch => "subject_capability_mismatch",
            Self::RemotePolicyMissing => "remote_policy_missing",
            Self::RemotePolicyUnknownAgent => "remote_policy_unknown_agent",
            Self::RemotePolicyExpired => "remote_policy_expired",
            Self::RemotePolicyRollback => "remote_policy_rollback",
            Self::RemotePolicyDenied => "remote_policy_denied",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityDecision {
    pub enabled: bool,
    pub source: DecisionSource,
    pub revision: Option<u64>,
    pub expires_at: Option<DateTime<Utc>>,
    pub denial_code: Option<DenialCode>,
}

pub fn evaluate(
    request: &CapabilityRequest,
    view: &PolicySnapshotView,
    now: DateTime<Utc>,
) -> CapabilityDecision {
    if let Some(code) = local_denial(request) {
        return deny(view, code);
    }
    if !subject_matches(request) {
        return deny(view, DenialCode::SubjectCapabilityMismatch);
    }
    evaluate_remote(request, view, now)
}

fn local_denial(request: &CapabilityRequest) -> Option<DenialCode> {
    if !request.compiled_support {
        return Some(DenialCode::CompiledSupportDisabled);
    }
    if !request.local_enabled {
        return Some(DenialCode::LocalPreferenceDisabled);
    }
    (!request.runtime_verified).then_some(DenialCode::RuntimeUnverified)
}

fn subject_matches(request: &CapabilityRequest) -> bool {
    matches!(
        (&request.subject, request.capability.is_agent_scoped()),
        (PolicySubject::Agent(_), true) | (PolicySubject::Client, false)
    )
}

fn evaluate_remote(
    request: &CapabilityRequest,
    view: &PolicySnapshotView,
    now: DateTime<Utc>,
) -> CapabilityDecision {
    let Some(snapshot) = view.snapshot.as_ref() else {
        return deny(view, DenialCode::RemotePolicyMissing);
    };
    if agent_policy_is_unknown(request, snapshot) {
        return deny(view, DenialCode::RemotePolicyUnknownAgent);
    }
    if view.source == PolicySnapshotSource::RevisionRollback {
        return evaluate_legacy_fallback(request, snapshot, view, DenialCode::RemotePolicyRollback);
    }
    if snapshot.expires_at <= now {
        return evaluate_legacy_fallback(request, snapshot, view, DenialCode::RemotePolicyExpired);
    }
    let Some(remote_allowed) = remote_allowed(request, snapshot) else {
        return deny(view, DenialCode::RemotePolicyUnknownAgent);
    };
    remote_allowed
        .then(|| allow(view))
        .unwrap_or_else(|| deny(view, DenialCode::RemotePolicyDenied))
}

fn agent_policy_is_unknown(
    request: &CapabilityRequest,
    snapshot: &CapabilityPolicySnapshot,
) -> bool {
    matches!(
        &request.subject,
        PolicySubject::Agent(agent) if snapshot.agent(&agent.platform_id).is_none()
    )
}

fn evaluate_legacy_fallback(
    request: &CapabilityRequest,
    snapshot: &CapabilityPolicySnapshot,
    view: &PolicySnapshotView,
    code: DenialCode,
) -> CapabilityDecision {
    if legacy_launch_is_allowed(request, snapshot) {
        return CapabilityDecision {
            enabled: true,
            source: DecisionSource::LegacyLastTrusted,
            revision: Some(snapshot.revision),
            expires_at: Some(snapshot.expires_at),
            denial_code: None,
        };
    }
    deny(view, code)
}

fn legacy_launch_is_allowed(
    request: &CapabilityRequest,
    snapshot: &CapabilityPolicySnapshot,
) -> bool {
    let PolicySubject::Agent(agent) = &request.subject else {
        return false;
    };
    agent.is_existing_agent
        && request.capability == Capability::AgentLaunch
        && !request.capability.is_sensitive()
        && snapshot
            .agent(&agent.platform_id)
            .is_some_and(|policy| policy.agent_allowed)
}

fn remote_allowed(
    request: &CapabilityRequest,
    snapshot: &CapabilityPolicySnapshot,
) -> Option<bool> {
    match &request.subject {
        PolicySubject::Agent(agent) => snapshot
            .agent(&agent.platform_id)
            .map(|policy| agent_allowed(request.capability, policy)),
        PolicySubject::Client => Some(client_allowed(request.capability, snapshot)),
    }
}

fn agent_allowed(capability: Capability, policy: &AgentCapabilityPolicy) -> bool {
    let base = policy.agent_allowed;
    match capability {
        Capability::AgentLaunch => base,
        Capability::HostExecution => base && policy.host_execution_allowed,
        Capability::HostRead => base && policy.host_execution_allowed && policy.host_read_allowed,
        Capability::HostWrite => base && policy.host_execution_allowed && policy.host_write_allowed,
        Capability::Terminal => base && policy.host_execution_allowed && policy.terminal_allowed,
        Capability::Mcp => base && policy.mcp_allowed,
        _ => false,
    }
}

fn client_allowed(capability: Capability, snapshot: &CapabilityPolicySnapshot) -> bool {
    match capability {
        Capability::FileUpload => snapshot.client.file_upload_allowed,
        Capability::ProjectBoot => snapshot.client.project_boot_allowed,
        Capability::FolderLinks => snapshot.client.folder_links_allowed,
        Capability::SplitView => snapshot.client.split_view_allowed,
        Capability::WorkTasks => snapshot.client.work_tasks_allowed,
        Capability::WorkTaskMerge => {
            snapshot.client.work_tasks_allowed && snapshot.client.work_task_merge_allowed
        }
        _ => false,
    }
}

fn allow(view: &PolicySnapshotView) -> CapabilityDecision {
    CapabilityDecision {
        enabled: true,
        source: decision_source(view.source),
        revision: view.snapshot.as_ref().map(|snapshot| snapshot.revision),
        expires_at: view.snapshot.as_ref().map(|snapshot| snapshot.expires_at),
        denial_code: None,
    }
}

fn deny(view: &PolicySnapshotView, code: DenialCode) -> CapabilityDecision {
    CapabilityDecision {
        enabled: false,
        source: decision_source(view.source),
        revision: view.snapshot.as_ref().map(|snapshot| snapshot.revision),
        expires_at: view.snapshot.as_ref().map(|snapshot| snapshot.expires_at),
        denial_code: Some(code),
    }
}

fn decision_source(source: PolicySnapshotSource) -> DecisionSource {
    match source {
        PolicySnapshotSource::Remote => DecisionSource::RemotePolicy,
        PolicySnapshotSource::TrustedCache | PolicySnapshotSource::RevisionRollback => {
            DecisionSource::TrustedCache
        }
        PolicySnapshotSource::Missing => DecisionSource::NoTrustedPolicy,
    }
}
