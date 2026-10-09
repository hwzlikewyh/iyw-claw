// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// How an action reaches a window, as the person has it set: said with every
/// listing, so an agent knows before it acts whether a window may be brought
/// to the front for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputPolicy {
    /// What an action gets when the agent does not ask (`delivery`).
    pub default: ActDelivery,
    /// Whether an agent may ask for the front.
    pub foreground_allowed: bool,
}
