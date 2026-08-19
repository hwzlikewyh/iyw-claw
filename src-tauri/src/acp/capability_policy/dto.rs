use std::collections::HashSet;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};

use super::error::CapabilityPolicyError;

pub const CAPABILITY_POLICY_SCHEMA_VERSION: u32 = 1;
const MAX_AGENT_POLICIES: usize = 128;
const MAX_PLATFORM_ID_BYTES: usize = 19;

#[derive(Debug, Clone, Copy)]
pub struct SnapshotValidationRules {
    pub max_future_clock_skew: Duration,
    pub max_validity: Duration,
}

impl Default for SnapshotValidationRules {
    fn default() -> Self {
        Self {
            max_future_clock_skew: Duration::from_secs(5 * 60),
            max_validity: Duration::from_secs(15 * 60),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityPolicySnapshot {
    pub schema_version: u32,
    pub revision: u64,
    pub generated_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub agents: Vec<AgentCapabilityPolicy>,
    pub client: ClientCapabilityPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentCapabilityPolicy {
    pub platform_id: String,
    pub agent_allowed: bool,
    pub host_execution_allowed: bool,
    pub host_read_allowed: bool,
    pub host_write_allowed: bool,
    pub terminal_allowed: bool,
    pub mcp_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientCapabilityPolicy {
    pub file_upload_allowed: bool,
    pub project_boot_allowed: bool,
    pub folder_links_allowed: bool,
    pub split_view_allowed: bool,
    pub work_tasks_allowed: bool,
    pub work_task_merge_allowed: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CapabilityPolicySnapshotWire {
    schema_version: u32,
    revision: u64,
    generated_at: String,
    expires_at: String,
    agents: Vec<AgentCapabilityPolicy>,
    client: ClientCapabilityPolicy,
}

impl<'de> Deserialize<'de> for CapabilityPolicySnapshot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = CapabilityPolicySnapshotWire::deserialize(deserializer)?;
        Self::from_wire(wire).map_err(serde::de::Error::custom)
    }
}

impl CapabilityPolicySnapshot {
    pub fn validate_at(
        &self,
        now: DateTime<Utc>,
        rules: SnapshotValidationRules,
    ) -> Result<(), CapabilityPolicyError> {
        self.validate_structure()?;
        validate_clock(self, now, rules)
    }

    pub fn validate_structure(&self) -> Result<(), CapabilityPolicyError> {
        if self.schema_version != CAPABILITY_POLICY_SCHEMA_VERSION {
            return invalid("unsupported schema version");
        }
        if self.agents.len() > MAX_AGENT_POLICIES {
            return invalid("too many agent policy entries");
        }
        validate_time_order(self)?;
        validate_agent_ids(&self.agents)
    }

    pub fn agent(&self, platform_id: &str) -> Option<&AgentCapabilityPolicy> {
        self.agents
            .iter()
            .find(|policy| policy.platform_id == platform_id)
    }

    pub fn same_permissions(&self, other: &Self) -> bool {
        self.client == other.client
            && self.agents.len() == other.agents.len()
            && self
                .agents
                .iter()
                .all(|policy| other.agent(&policy.platform_id) == Some(policy))
    }

    fn from_wire(wire: CapabilityPolicySnapshotWire) -> Result<Self, String> {
        let generated_at = parse_time("generatedAt", &wire.generated_at)?;
        let expires_at = parse_time("expiresAt", &wire.expires_at)?;
        let snapshot = Self {
            schema_version: wire.schema_version,
            revision: wire.revision,
            generated_at,
            expires_at,
            agents: wire.agents,
            client: wire.client,
        };
        snapshot
            .validate_structure()
            .map_err(|error| error.to_string())?;
        Ok(snapshot)
    }
}

fn validate_time_order(snapshot: &CapabilityPolicySnapshot) -> Result<(), CapabilityPolicyError> {
    if snapshot.expires_at <= snapshot.generated_at {
        return invalid("expiresAt must be after generatedAt");
    }
    Ok(())
}

fn validate_agent_ids(entries: &[AgentCapabilityPolicy]) -> Result<(), CapabilityPolicyError> {
    let mut seen = HashSet::with_capacity(entries.len());
    for entry in entries {
        if !valid_platform_id(&entry.platform_id) {
            return invalid("platformId must be a positive decimal int64 string");
        }
        if !seen.insert(entry.platform_id.as_str()) {
            return invalid("duplicate platformId in agent policy");
        }
    }
    Ok(())
}

fn valid_platform_id(value: &str) -> bool {
    value.len() <= MAX_PLATFORM_ID_BYTES
        && !value.starts_with('0')
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && value.parse::<i64>().is_ok_and(|id| id > 0)
}

fn validate_clock(
    snapshot: &CapabilityPolicySnapshot,
    now: DateTime<Utc>,
    rules: SnapshotValidationRules,
) -> Result<(), CapabilityPolicyError> {
    let future_limit = now
        + chrono::Duration::from_std(rules.max_future_clock_skew).map_err(|_| {
            CapabilityPolicyError::InvalidSnapshot("clock skew is too large".into())
        })?;
    let max_validity = chrono::Duration::from_std(rules.max_validity).map_err(|_| {
        CapabilityPolicyError::InvalidSnapshot("validity window is too large".into())
    })?;
    if snapshot.generated_at > future_limit {
        return invalid("generatedAt exceeds the permitted clock skew");
    }
    if snapshot.expires_at - snapshot.generated_at > max_validity {
        return invalid("expiresAt exceeds the permitted validity window");
    }
    if snapshot.expires_at <= now {
        return invalid("capability policy is already expired");
    }
    Ok(())
}

fn parse_time(name: &str, value: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(value)
        .map(|time| time.with_timezone(&Utc))
        .map_err(|_| format!("{name} must be RFC3339"))
}

fn invalid(message: &str) -> Result<(), CapabilityPolicyError> {
    Err(CapabilityPolicyError::InvalidSnapshot(message.to_string()))
}
