use std::path::Path;

use serde::{Deserialize, Serialize};
use sysinfo::{Pid, System};

use crate::acp::builtin_agent_prompt::RenderedBuiltinPrompt;
use crate::acp::builtin_prompt_bridge_file::ensure_plain_path;
use crate::acp::error::AcpError;
use crate::models::agent::AgentType;

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct LeaseRecord {
    pid: u32,
    process_start_time: u64,
    connection_id: String,
    agent_type: AgentType,
    prompt_hash: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(super) struct LeaseState {
    pub leases: Vec<LeaseRecord>,
}

impl LeaseState {
    pub fn read(path: &Path) -> Result<Self, AcpError> {
        ensure_plain_path(path)?;
        let raw = crate::acp::provider_overlay_files::read_optional(path)
            .map_err(AcpError::BuiltinPromptInjection)?;
        if raw.trim().is_empty() {
            return Ok(Self::default());
        }
        serde_json::from_str(&raw).map_err(|error| {
            AcpError::BuiltinPromptInjection(format!(
                "invalid prompt bridge lease {}: {error}",
                path.display()
            ))
        })
    }

    pub fn write(&self, path: &Path) -> Result<(), AcpError> {
        ensure_plain_path(path)?;
        let old = crate::acp::provider_overlay_files::read_optional(path)
            .map_err(AcpError::BuiltinPromptInjection)?;
        let next = serde_json::to_string_pretty(self)
            .map_err(|error| AcpError::BuiltinPromptInjection(error.to_string()))?
            + "\n";
        crate::acp::provider_overlay_files::write_if_changed(path, &old, &next)
            .map_err(AcpError::BuiltinPromptInjection)
    }

    pub fn prune_dead(&mut self) {
        let system = System::new_all();
        self.leases.retain(|lease| {
            system
                .process(Pid::from_u32(lease.pid))
                .is_some_and(|process| process.start_time() == lease.process_start_time)
        });
    }

    pub fn reject_conflicting(&self, prompt_hash: &str, slug: &str) -> Result<(), AcpError> {
        if self
            .leases
            .iter()
            .any(|lease| lease.prompt_hash != prompt_hash)
        {
            return Err(AcpError::BuiltinPromptBridgeBusy(format!(
                "{slug} is used by a live connection with a different prompt version"
            )));
        }
        Ok(())
    }

    pub fn remove_connection(&mut self, connection_id: &str) {
        self.leases
            .retain(|lease| lease.connection_id != connection_id);
    }

    pub fn upsert(
        &mut self,
        connection_id: &str,
        prompt: &RenderedBuiltinPrompt,
        agent_type: AgentType,
    ) -> Result<(), AcpError> {
        let process_start_time = current_process_start_time().ok_or_else(|| {
            AcpError::BuiltinPromptInjection(
                "failed to resolve the current process start time".to_string(),
            )
        })?;
        self.remove_connection(connection_id);
        self.leases.push(LeaseRecord {
            pid: std::process::id(),
            process_start_time,
            connection_id: connection_id.to_string(),
            agent_type,
            prompt_hash: prompt.hash.clone(),
        });
        Ok(())
    }
}

fn current_process_start_time() -> Option<u64> {
    System::new_all()
        .process(Pid::from_u32(std::process::id()))
        .map(|process| process.start_time())
}
