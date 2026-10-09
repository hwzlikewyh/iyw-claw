// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl HelperError {
    pub fn new(code: HelperErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            permission: None,
        }
    }

    pub fn failed(message: impl Into<String>) -> Self {
        Self::new(HelperErrorCode::Failed, message)
    }

    pub fn permission_missing(permission: OsPermission) -> Self {
        let what = match permission {
            OsPermission::Accessibility => "Accessibility",
            OsPermission::ScreenRecording => "Screen Recording",
        };
        Self {
            code: HelperErrorCode::PermissionMissing,
            message: format!("iyw-computer-helper has not been granted {what}"),
            permission: Some(permission),
        }
    }
}

impl std::fmt::Display for HelperError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
