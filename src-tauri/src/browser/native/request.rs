use std::time::Duration;

use serde_json::Value;
use tokio_util::sync::CancellationToken;

pub(super) struct NativeRequest<'a> {
    pub id: &'a str,
    pub action: &'a str,
    pub values: Value,
    pub timeout: Duration,
    pub cancellation: CancellationToken,
}

impl<'a> NativeRequest<'a> {
    pub(super) fn new(id: &'a str, action: &'a str, values: Value) -> Self {
        Self {
            id,
            action,
            values,
            timeout: Duration::from_secs(30),
            cancellation: CancellationToken::new(),
        }
    }

    pub(super) fn within(mut self, timeout: Duration, cancellation: CancellationToken) -> Self {
        self.timeout = timeout;
        self.cancellation = cancellation;
        self
    }
}

pub(in crate::browser) struct NativeExecution<'a> {
    pub id: &'a str,
    pub args: &'a [String],
    pub timeout: Duration,
    pub cancellation: CancellationToken,
}

impl NativeExecution<'_> {
    pub(super) fn request<'a>(&'a self, action: &'a str, values: Value) -> NativeRequest<'a> {
        NativeRequest::new(self.id, action, values).within(self.timeout, self.cancellation.clone())
    }
}
