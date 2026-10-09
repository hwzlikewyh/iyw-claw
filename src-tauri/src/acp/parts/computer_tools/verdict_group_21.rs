// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl ComputerVerifyOutcome {
    pub fn verdict(target_id: &str, verify: VerifyOutcome) -> Self {
        Self {
            target_id: target_id.to_string(),
            verify: Some(verify),
            error: None,
            note: None,
        }
    }

    pub fn refused(target_id: &str, error: &str, note: impl Into<String>) -> Self {
        Self {
            target_id: target_id.to_string(),
            verify: None,
            error: Some(error.to_string()),
            note: Some(note.into()),
        }
    }
}
