use std::time::{Duration, Instant};

use serde_json::Value;

use super::super::{execute_kind, limits::OUTPUT_RETRY_DELAYS_SECONDS};
use super::{output, ImageExecution, ImageResult, IywGatewayService};

pub(super) struct Candidate<'a> {
    pub index: usize,
    pub attempts: &'a mut Vec<Value>,
}

pub(super) async fn execute(
    service: &IywGatewayService,
    execution: ImageExecution<'_>,
    context: Candidate<'_>,
) -> (Result<ImageResult, rmcp::ErrorData>, &'static str) {
    for retry_index in 0..=OUTPUT_RETRY_DELAYS_SECONDS.len() {
        let started = Instant::now();
        let result = execute_kind(service, execution).await;
        let delay = retry_delay(execution.kind, &result, retry_index);
        let decision = if delay.is_some() {
            "retry_same_model"
        } else {
            output::decision(&result)
        };
        let attempt = output::Attempt {
            index: context.attempts.len(),
            candidate_index: context.index,
            retry_index,
            retry_delay_seconds: delay,
            kind: execution.kind,
            duration_ms: started.elapsed().as_millis(),
            decision,
        };
        log_attempt(&attempt);
        context.attempts.push(output::attempt(&result, attempt));
        let Some(delay) = delay else {
            return (result, decision);
        };
        if let Err(error) = service
            .image_io("retry_wait", async {
                tokio::time::sleep(Duration::from_secs(delay)).await;
                Ok(())
            })
            .await
        {
            return (Err(error), "stop_uncertain");
        }
    }
    unreachable!("the retry limit always ends the candidate")
}

fn retry_delay(
    kind: &str,
    result: &Result<ImageResult, rmcp::ErrorData>,
    index: usize,
) -> Option<u64> {
    if !matches!(kind, "generate" | "edit") {
        return None;
    }
    let data = result.as_ref().err()?.data.as_ref()?;
    let code = data.get("error_code").and_then(Value::as_str);
    let confirmed_output_failure = data.get("status").and_then(Value::as_u64) == Some(502)
        && data.get("execution_status").and_then(Value::as_str) == Some("failed")
        && data.get("retryable").and_then(Value::as_bool) == Some(true)
        && matches!(
            code,
            Some("upstream_image_download_failed" | "image_persist_failed")
        );
    confirmed_output_failure
        .then(|| OUTPUT_RETRY_DELAYS_SECONDS.get(index).copied())
        .flatten()
}

fn log_attempt(attempt: &output::Attempt<'_>) {
    tracing::info!(
        attempt_index = attempt.index,
        candidate_index = attempt.candidate_index,
        retry_index = attempt.retry_index,
        image_type = attempt.kind,
        decision = attempt.decision,
        duration_ms = attempt.duration_ms,
        retry_delay_seconds = attempt.retry_delay_seconds,
        "[iyw-image] recovery attempt completed"
    );
}
