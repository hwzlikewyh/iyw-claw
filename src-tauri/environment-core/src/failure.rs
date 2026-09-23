use std::fmt;

const PERMISSION_DENIED_EXIT_CODE: u8 = 31;

#[derive(Debug)]
pub struct Failure {
    pub code: &'static str,
    pub message: String,
    pub retryable: bool,
}

impl Failure {
    pub fn network(message: impl Into<String>) -> Self {
        Self {
            code: "NETWORK",
            message: message.into(),
            retryable: true,
        }
    }

    pub fn permanent(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retryable: false,
        }
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for Failure {}

pub fn network(error: reqwest::Error) -> anyhow::Error {
    Failure::network(format!(
        "网络请求失败，请检查网络或代理：{}",
        error.without_url()
    ))
    .into()
}

pub fn exit_code(error: &anyhow::Error) -> u8 {
    if error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|error| error.kind() == std::io::ErrorKind::PermissionDenied)
    }) {
        return PERMISSION_DENIED_EXIT_CODE;
    }
    match error.downcast_ref::<Failure>().map(|error| error.code) {
        Some("NETWORK") => 20,
        Some("INTEGRITY") => 21,
        Some("UNSUPPORTED") => 22,
        Some("BUSY") => 11,
        Some("PLAN") => 24,
        _ => 30,
    }
}
