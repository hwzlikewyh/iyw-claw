// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// What `computer_list_windows` answers.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerWindowsOutcome {
    pub windows: Vec<AgentWindowSummary>,
    /// The entire screen, while the user shares it as a whole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen: Option<AgentScreen>,
    /// How actions reach the windows, as the person has it set now.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<InputPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}
