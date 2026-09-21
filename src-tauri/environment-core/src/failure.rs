use std::fmt;

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
    match error.downcast_ref::<Failure>().map(|error| error.code) {
        Some("NETWORK") => 20,
        Some("INTEGRITY") => 21,
        Some("UNSUPPORTED") => 22,
        Some("BUSY") => 11,
        Some("PLAN") => 24,
        _ => 30,
    }
}
