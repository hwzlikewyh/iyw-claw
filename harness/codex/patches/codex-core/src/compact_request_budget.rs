use std::future::Future;
use std::time::Duration;

use codex_protocol::error::CodexErr;
use codex_protocol::error::Result as CodexResult;
use tokio::time::Instant;

const COMPACTION_REQUEST_BUDGET: Duration = Duration::from_secs(300);

pub(crate) struct RequestBudget {
    deadline: Instant,
}

impl RequestBudget {
    pub(crate) fn new() -> Self {
        Self {
            deadline: Instant::now() + COMPACTION_REQUEST_BUDGET,
        }
    }

    pub(crate) fn is_exhausted(&self) -> bool {
        Instant::now() >= self.deadline
    }

    pub(crate) fn retry_delay(&self, delay: Duration) -> Duration {
        delay.min(self.deadline.saturating_duration_since(Instant::now()))
    }

    pub(crate) async fn run<T>(
        &self,
        request: impl Future<Output = CodexResult<T>>,
    ) -> CodexResult<T> {
        if self.is_exhausted() {
            return Err(self.exhausted_error());
        }
        // 仅约束请求与重试，不在成功后的历史提交中途取消。
        match tokio::time::timeout_at(self.deadline, request).await {
            Ok(result) => result,
            Err(_) => Err(self.exhausted_error()),
        }
    }

    fn exhausted_error(&self) -> CodexErr {
        tracing::warn!(
            budget_seconds = COMPACTION_REQUEST_BUDGET.as_secs(),
            "compaction request budget exhausted; stopping retries"
        );
        CodexErr::Stream(format!(
            "Context compaction exceeded its {} second request budget; retries stopped before replacing history.",
            COMPACTION_REQUEST_BUDGET.as_secs()
        ))
    }
}
