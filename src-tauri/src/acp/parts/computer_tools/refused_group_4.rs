// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl ComputerAppsOutcome {
    pub fn refused(error: &str, note: impl Into<String>) -> Self {
        Self {
            apps: Vec::new(),
            error: Some(error.to_string()),
            note: Some(note.into()),
        }
    }
}
