// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// What `computer_verify` answers.
///
/// Carries no observed values, only verdicts: the point of the tool is to
/// answer "is it so yet" without handing back what is on the screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyOutcome {
    pub target_id: String,
    pub status: VerifyStatus,
    pub stable: bool,
    pub samples: u64,
    pub elapsed_ms: u64,
    pub predicates: Vec<PredicateResult>,
}

// -------- Acting on a window ----------------------------------------------

/// An element of the window's latest snapshot: the `generation` that snapshot
/// handed out, and the element's ref — the number in `[N]` at the start of
/// its line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ElementTarget {
    pub generation: String,
    #[serde(rename = "ref")]
    pub index: u32,
}

/// A point in the window's latest screenshot, in that image's pixels, with
/// the `generation` it handed out. Only that image's pixel space is meant:
/// the same numbers read off another capture are another point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PointTarget {
    pub generation: String,
    pub x: f64,
    pub y: f64,
}

/// Where a pointer action lands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "at", rename_all = "camelCase")]
pub enum AgentTarget {
    Element(ElementTarget),
    Point(PointTarget),
}
