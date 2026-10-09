// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl ComputerActOutcome {
    pub fn done(target_id: &str, report: ActReport) -> Self {
        Self {
            target_id: target_id.to_string(),
            action: Some(report),
            error: None,
            note: None,
        }
    }

    pub fn refused(target_id: &str, error: &str, note: impl Into<String>) -> Self {
        Self {
            target_id: target_id.to_string(),
            action: None,
            error: Some(error.to_string()),
            note: Some(note.into()),
        }
    }
}
