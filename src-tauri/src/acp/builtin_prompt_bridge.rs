use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

use crate::acp::agent_storage::AgentStoragePaths;
use crate::acp::builtin_agent_prompt::RenderedBuiltinPrompt;
use crate::acp::builtin_prompt_bridge_file::{
    ensure_plain_path, remove_managed_block, upsert_managed_block,
};
use crate::acp::builtin_prompt_bridge_state::LeaseState;
use crate::acp::error::AcpError;
use crate::models::agent::AgentType;

#[derive(Debug, Clone)]
struct BridgeSpec {
    slug: &'static str,
    target: PathBuf,
    agent_type: AgentType,
}

pub struct PrepareRequest<'a> {
    pub agent_type: AgentType,
    pub connection_id: &'a str,
    pub prompt: &'a RenderedBuiltinPrompt,
    pub storage: &'a AgentStoragePaths,
}

#[derive(Debug)]
pub struct PromptBridgeLease {
    spec: BridgeSpec,
    lock_path: PathBuf,
    state_path: PathBuf,
    connection_id: String,
    prompt_hash: String,
}

#[derive(Debug, Default)]
pub struct PreparedPromptBridges {
    leases: Vec<PromptBridgeLease>,
    pub opencode_instruction: Option<PathBuf>,
}

impl PreparedPromptBridges {
    pub fn prepare(request: PrepareRequest<'_>) -> Result<Self, AcpError> {
        let Some(spec) = bridge_spec(request.agent_type, request.storage)? else {
            return Ok(Self::default());
        };
        let opencode_instruction =
            (request.agent_type == AgentType::OpenCode).then(|| spec.target.clone());
        let lease = PromptBridgeLease::acquire(spec, &request)?;
        Ok(Self {
            leases: vec![lease],
            opencode_instruction,
        })
    }

    pub fn release(&mut self) -> Result<(), AcpError> {
        let mut errors = Vec::new();
        let mut failed = Vec::new();
        for lease in self.leases.drain(..) {
            if let Err(error) = lease.release() {
                errors.push(error.to_string());
                failed.push(lease);
            }
        }
        self.leases = failed;
        if errors.is_empty() {
            Ok(())
        } else {
            Err(AcpError::BuiltinPromptBridgeCleanup(errors.join("; ")))
        }
    }
}

pub(super) fn cleanup_stale(storage: &AgentStoragePaths) -> Result<(), AcpError> {
    let agents = [
        AgentType::Gemini,
        AgentType::Cline,
        AgentType::OpenCode,
        AgentType::KimiCode,
    ];
    let mut errors = Vec::new();
    for agent_type in agents {
        let result = bridge_spec(agent_type, storage).and_then(|spec| {
            spec.map_or(Ok(()), |spec| {
                cleanup_stale_spec(storage, &spec).map_err(cleanup_error)
            })
        });
        if let Err(error) = result {
            errors.push(format!("{agent_type}: {error}"));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(AcpError::BuiltinPromptBridgeCleanup(errors.join("; ")))
    }
}

fn cleanup_stale_spec(storage: &AgentStoragePaths, spec: &BridgeSpec) -> Result<(), AcpError> {
    let (lock_path, state_path) = bridge_metadata_paths(storage, spec.slug);
    let lock = acquire_lock(&lock_path)?;
    let mut state = LeaseState::read(&state_path)?;
    let previous_count = state.leases.len();
    state.prune_dead();
    if state.leases.is_empty() {
        remove_managed_block(&spec.target, None)?;
        if previous_count > 0 {
            state.write(&state_path)?;
        }
    }
    drop(lock);
    Ok(())
}

impl Drop for PreparedPromptBridges {
    fn drop(&mut self) {
        if let Err(error) = self.release() {
            tracing::warn!(
                code = ?error.code(),
                error = %error,
                "[ACP] built-in prompt bridge cleanup failed during drop"
            );
        }
    }
}

impl PromptBridgeLease {
    fn acquire(spec: BridgeSpec, request: &PrepareRequest<'_>) -> Result<Self, AcpError> {
        let (lock_path, state_path) = bridge_metadata_paths(request.storage, spec.slug);
        let lock = acquire_lock(&lock_path)?;
        let mut state = LeaseState::read(&state_path)?;
        state.prune_dead();
        state.reject_conflicting(&request.prompt.hash, spec.slug)?;
        let had_live_leases = !state.leases.is_empty();
        state.upsert(request.connection_id, request.prompt, spec.agent_type)?;
        upsert_managed_block(&spec.target, request.prompt)?;
        if let Err(error) = state.write(&state_path) {
            if !had_live_leases {
                let _ = remove_managed_block(&spec.target, Some(&request.prompt.hash));
            }
            return Err(error);
        }
        drop(lock);
        Ok(Self {
            spec,
            lock_path,
            state_path,
            connection_id: request.connection_id.to_string(),
            prompt_hash: request.prompt.hash.clone(),
        })
    }

    fn release(&self) -> Result<(), AcpError> {
        let lock = acquire_lock(&self.lock_path).map_err(cleanup_error)?;
        let mut state = LeaseState::read(&self.state_path).map_err(cleanup_error)?;
        state.prune_dead();
        state.remove_connection(&self.connection_id);
        state.write(&self.state_path).map_err(cleanup_error)?;
        if state.leases.is_empty() {
            remove_managed_block(&self.spec.target, Some(&self.prompt_hash))
                .map_err(cleanup_error)?;
        }
        drop(lock);
        Ok(())
    }
}

fn bridge_spec(
    agent_type: AgentType,
    storage: &AgentStoragePaths,
) -> Result<Option<BridgeSpec>, AcpError> {
    let profile = || {
        crate::acp::provider_overlay_files::active_profile_root(agent_type)
            .map_err(AcpError::BuiltinPromptInjection)
    };
    let spec = match agent_type {
        AgentType::Gemini => BridgeSpec {
            slug: "gemini",
            target: profile()?.join("GEMINI.md"),
            agent_type,
        },
        AgentType::Cline => BridgeSpec {
            slug: "cline",
            target: profile()?.join("rules").join("iyw-claw-builtin.md"),
            agent_type,
        },
        AgentType::OpenCode => BridgeSpec {
            slug: "opencode",
            target: storage
                .runtime_dir()
                .join("builtin-prompts")
                .join("shared")
                .join("opencode.md"),
            agent_type,
        },
        AgentType::KimiCode => BridgeSpec {
            slug: "kimi-code",
            target: profile()?.join("AGENTS.md"),
            agent_type,
        },
        _ => return Ok(None),
    };
    Ok(Some(spec))
}

fn bridge_metadata_paths(storage: &AgentStoragePaths, slug: &str) -> (PathBuf, PathBuf) {
    let root = storage.runtime_dir().join("builtin-prompts").join("leases");
    (
        root.join(format!("{slug}.lock")),
        root.join(format!("{slug}.json")),
    )
}

fn acquire_lock(path: &Path) -> Result<File, AcpError> {
    ensure_plain_path(path)?;
    let parent = path.parent().ok_or_else(|| {
        AcpError::BuiltinPromptInjection(format!("{} has no parent", path.display()))
    })?;
    fs::create_dir_all(parent).map_err(injection_io(path))?;
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(path)
        .map_err(injection_io(path))?;
    file.lock().map_err(injection_io(path))?;
    Ok(file)
}

fn injection_io(path: &Path) -> impl FnOnce(std::io::Error) -> AcpError + '_ {
    move |error| AcpError::BuiltinPromptInjection(format!("{}: {error}", path.display()))
}

fn cleanup_error(error: AcpError) -> AcpError {
    AcpError::BuiltinPromptBridgeCleanup(error.to_string())
}
