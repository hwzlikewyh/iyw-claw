// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// What iyw-claw knows about its own TCC standing (macOS only). Shown to the
/// person, never acted on: a permission iyw-claw itself holds is one every
/// agent's shell holds too, outside anything computer use decides — refusing
/// to run would not take it back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostTccStatus {
    pub accessibility: bool,
    pub screen_recording: bool,
    /// Whether iyw-claw is its own responsible process. When it is not (a
    /// development build run from a terminal), the two flags above are the
    /// terminal's, which every process in that terminal already has.
    pub self_responsible: bool,
}
