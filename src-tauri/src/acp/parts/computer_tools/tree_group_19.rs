// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl ComputerSnapshotOutcome {
    pub fn tree(target_id: &str, snapshot: WindowSnapshot) -> Self {
        Self {
            target_id: target_id.to_string(),
            snapshot: Some(snapshot),
            error: None,
            note: None,
        }
    }

    pub fn refused(target_id: &str, error: &str, note: impl Into<String>) -> Self {
        Self {
            target_id: target_id.to_string(),
            snapshot: None,
            error: Some(error.to_string()),
            note: Some(note.into()),
        }
    }
}
