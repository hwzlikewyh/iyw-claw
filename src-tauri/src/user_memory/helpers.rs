use std::collections::BTreeMap;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::app_error::{AppCommandError, AppErrorCode};
use crate::models::agent::AgentType;

use super::context::{USER_CONTEXT_END, USER_CONTEXT_START};
use super::{
    UserMemoryContextSnapshot, UserMemoryDocumentId, UserMemoryDocumentSnapshot, UserMemoryOrigin,
    UserMemoryPolicy, UserMemorySettingsSnapshot, UserMemoryUpdateRequest,
    USER_MEMORY_MAX_APPEND_CHARS, USER_MEMORY_MAX_DOCUMENT_CHARS,
};

pub(super) fn apply_policy_patch(policy: &mut UserMemoryPolicy, request: &UserMemoryUpdateRequest) {
    if let Some(value) = request.enabled {
        policy.enabled = value;
    }
    if let Some(value) = request.agent_write_enabled {
        policy.agent_write_enabled = value;
    }
    if let Some(value) = request.inherit_to_subagents {
        policy.inherit_to_subagents = value;
    }
    if let Some(values) = &request.per_agent {
        policy
            .per_agent
            .extend(values.iter().map(|(key, value)| (*key, *value)));
    }
    for (id, patch) in &request.documents {
        if let Some(value) = patch.enabled {
            policy.documents.insert(*id, value);
        }
    }
}

pub(super) fn policy_from_snapshot(snapshot: &UserMemorySettingsSnapshot) -> UserMemoryPolicy {
    UserMemoryPolicy {
        enabled: snapshot.enabled,
        agent_write_enabled: snapshot.agent_write_enabled,
        inherit_to_subagents: snapshot.inherit_to_subagents,
        per_agent: snapshot.per_agent.clone(),
        documents: snapshot
            .documents
            .iter()
            .map(|(id, document)| (*id, document.enabled))
            .collect(),
    }
}

pub(super) fn settings_revision(
    policy: &UserMemoryPolicy,
    documents: &BTreeMap<UserMemoryDocumentId, UserMemoryDocumentSnapshot>,
) -> Result<String, AppCommandError> {
    let policy = serde_json::to_vec(policy)
        .map_err(|error| AppCommandError::configuration_invalid(error.to_string()))?;
    let mut parts = vec![policy.as_slice()];
    for id in UserMemoryDocumentId::ALL {
        parts.push(documents[&id].etag.as_bytes());
    }
    Ok(hash_parts(&parts))
}

pub(super) fn normalize_append(input: &str) -> Result<String, AppCommandError> {
    if input.contains('\0') {
        return Err(AppCommandError::invalid_input("Memory contains NUL"));
    }
    let content = input.split_whitespace().collect::<Vec<_>>().join(" ");
    if content.is_empty() {
        return Err(AppCommandError::invalid_input("Memory is empty"));
    }
    if content.contains("<!--") || content.contains("-->") {
        return Err(AppCommandError::invalid_input(
            "Memory cannot contain HTML comment markers",
        ));
    }
    if content.chars().count() > USER_MEMORY_MAX_APPEND_CHARS {
        return Err(AppCommandError::invalid_input("Memory entry is too long"));
    }
    if contains_potential_secret(&content) {
        return Err(AppCommandError::invalid_input(
            "Potential secrets cannot be stored in user memory",
        ));
    }
    Ok(content)
}

fn contains_potential_secret(content: &str) -> bool {
    let lower = content.to_ascii_lowercase();
    let markers = [
        "password",
        "api key",
        "api_key",
        "private key",
        "access token",
        "bearer ",
        "ghp_",
        "github_pat_",
        "client_secret",
        "client-secret",
        "secret=",
        "secret:",
        "token=",
        "token:",
        "-----begin",
        "sk-",
        "密码",
        "口令",
    ];
    markers.iter().any(|marker| lower.contains(marker))
        || content.split_whitespace().any(looks_like_jwt)
        || contains_aws_access_key(content.as_bytes())
}

fn looks_like_jwt(value: &str) -> bool {
    let token = value.trim_matches(|character: char| {
        !character.is_ascii_alphanumeric() && !matches!(character, '-' | '_' | '.')
    });
    let parts = token.split('.').collect::<Vec<_>>();
    parts.len() == 3
        && parts[0].starts_with("eyJ")
        && parts.iter().all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        })
}

fn contains_aws_access_key(bytes: &[u8]) -> bool {
    bytes.windows(20).any(|window| {
        window.starts_with(b"AKIA")
            && window[4..]
                .iter()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
    })
}

pub(super) fn validate_document_content(content: &str) -> Result<(), AppCommandError> {
    if content.contains('\0') {
        return Err(AppCommandError::invalid_input("User memory contains NUL"));
    }
    if content.chars().count() > USER_MEMORY_MAX_DOCUMENT_CHARS {
        return Err(AppCommandError::invalid_input(
            "User memory document is too large",
        ));
    }
    Ok(())
}

pub(super) fn validate_document_update_content(content: &str) -> Result<(), AppCommandError> {
    validate_document_content(content)?;
    if content.contains(USER_CONTEXT_START) || content.contains(USER_CONTEXT_END) {
        return Err(AppCommandError::invalid_input(
            "User memory cannot contain private context markers",
        ));
    }
    Ok(())
}

pub(super) fn ensure_agent_write_allowed(
    policy: &UserMemoryPolicy,
    agent: AgentType,
) -> Result<(), AppCommandError> {
    let allowed = policy.enabled
        && policy.agent_write_enabled
        && policy.per_agent.get(&agent).copied().unwrap_or(true)
        && policy
            .documents
            .get(&UserMemoryDocumentId::Memory)
            .copied()
            .unwrap_or(true)
        && supports_memory_tool(agent);
    if allowed {
        Ok(())
    } else {
        Err(AppCommandError::permission_denied(
            "Agent memory updates are disabled",
        ))
    }
}

pub(super) fn supports_memory_tool(agent: AgentType) -> bool {
    !matches!(agent, AgentType::OpenClaw | AgentType::Pi)
}

pub(super) fn reject_symlink(path: &Path) -> Result<(), AppCommandError> {
    let mut current = Some(path);
    while let Some(candidate) = current {
        match std::fs::symlink_metadata(candidate) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(AppCommandError::permission_denied(
                    "User memory paths cannot contain symlinks",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(AppCommandError::io(error)),
        }
        current = candidate.parent();
    }
    Ok(())
}

pub(super) fn conflict(message: &str) -> AppCommandError {
    AppCommandError::new(AppErrorCode::Conflict, message)
}

pub(super) fn disabled_with_fingerprint(
    origin: UserMemoryOrigin,
    revision: &str,
) -> UserMemoryContextSnapshot {
    UserMemoryContextSnapshot {
        revision: revision.to_string(),
        effective_fingerprint: hash_parts(&[b"disabled"]),
        rendered: None,
        memory_write_enabled: false,
        origin,
    }
}

pub(super) fn hash_parts(parts: &[&[u8]]) -> String {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update(part);
        hash.update([0]);
    }
    format!("{:x}", hash.finalize())
}
