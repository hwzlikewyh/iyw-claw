// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// What one action on a shared window did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActReport {
    pub target_id: String,
    pub effect: ActEffect,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route: Option<ActRoute>,
    pub delivery: ActDelivery,
    /// For a key pressed more than once: how many presses went out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presses: Option<u32>,
    /// For typing with `submit`: whether return was pressed after it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub submitted: Option<bool>,
    /// For typing with `submit` whose return was not pressed: why.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submit_note: Option<String>,
}
