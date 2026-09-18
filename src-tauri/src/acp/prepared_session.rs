use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::models::AgentType;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PrepareSessionRequest {
    pub agent_type: AgentType,
    pub working_dir: Option<String>,
    pub session_id: Option<String>,
    pub conversation_id: Option<i32>,
    pub preferred_mode_id: Option<String>,
    #[serde(default)]
    pub preferred_config_values: BTreeMap<String, String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedSessionHandle {
    pub id: String,
    pub working_dir: String,
}
