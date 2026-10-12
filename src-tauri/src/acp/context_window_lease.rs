//! 只提供共享配置文件的 SDK 不能同时加载冲突的窗口预算。
use std::collections::{BTreeMap, HashMap};
use std::sync::{LazyLock, Mutex};

use super::error::AcpError;
use crate::models::AgentType;

static LEASES: LazyLock<Mutex<HashMap<AgentType, (String, usize)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) struct ContextWindowLease {
    agent: AgentType,
}

pub(crate) fn acquire(
    agent: AgentType,
    preferences: &BTreeMap<String, String>,
) -> Result<Option<ContextWindowLease>, AcpError> {
    if matches!(
        agent,
        AgentType::Codex | AgentType::ClaudeCode | AgentType::OpenCode | AgentType::CodeBuddy
    ) || !super::context_window_runtime::supports(agent)
    {
        return Ok(None);
    }
    let model = preferences
        .get("model")
        .map(String::as_str)
        .unwrap_or_else(|| super::model_catalog::default_model_for(agent));
    if super::model_catalog::model_capabilities_for(agent, model)
        .and_then(|value| value.limits.context_window)
        .is_none()
    {
        return Ok(None);
    }
    let window = preferences
        .get(super::context_window_config::CONFIG_ID)
        .map(String::as_str)
        .unwrap_or("auto");
    let signature = format!(
        "{model}:{window}:{}",
        super::model_catalog::snapshot_for(agent)
    );
    let mut leases = LEASES
        .lock()
        .map_err(|_| AcpError::protocol("SDK context configuration lease is unavailable"))?;
    if leases
        .get(&agent)
        .is_some_and(|(current, _)| current != &signature)
    {
        tracing::warn!(agent = %agent, model, window, "[ACP] rejected conflicting shared SDK context configuration");
        return Err(AcpError::protocol("This SDK uses a shared native configuration. Close its other active sessions before applying a different model context window."));
    }
    let lease = leases.entry(agent).or_insert((signature, 0));
    lease.1 += 1;
    Ok(Some(ContextWindowLease { agent }))
}

impl Drop for ContextWindowLease {
    fn drop(&mut self) {
        if let Ok(mut leases) = LEASES.lock() {
            if let Some((_, count)) = leases.get_mut(&self.agent) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    leases.remove(&self.agent);
                }
            }
        }
    }
}
