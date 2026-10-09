// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl ComputerLaunchOutcome {
    pub fn refused(error: &str, note: impl Into<String>) -> Self {
        Self {
            app: None,
            error: Some(error.to_string()),
            note: Some(note.into()),
        }
    }
}
