use std::time::Instant;

use codex_api::{ApiError, ResponseEvent};

pub(super) struct ModelRequestTiming {
    id: uuid::Uuid,
    thread_id: String,
    started: Instant,
    first_event: bool,
    first_content: bool,
    finished: bool,
}

impl ModelRequestTiming {
    pub(super) fn new(thread_id: String, transport: &'static str) -> Self {
        let timing = Self {
            id: uuid::Uuid::new_v4(),
            thread_id,
            started: Instant::now(),
            first_event: false,
            first_content: false,
            finished: false,
        };
        tracing::info!(request_trace = %timing.id, thread_id = timing.thread_id,
            transport, stage = "request_ready", "[model-timing] stage");
        timing
    }

    pub(super) fn opened(&self, request_id: Option<&str>) {
        // 只记录有界关联标识，不能记录响应头、请求正文或凭证。
        let request_id = request_id.filter(|id| {
            id.len() <= 128
                && id
                    .bytes()
                    .all(|ch| ch.is_ascii_alphanumeric() || b"-_.:".contains(&ch))
        });
        tracing::info!(request_trace = %self.id, thread_id = self.thread_id,
            request_id, stage = "stream_open", elapsed_ms = self.started.elapsed().as_millis(),
            "[model-timing] stage");
    }

    pub(super) fn observe(&mut self, event: &Result<ResponseEvent, ApiError>) {
        if !self.first_event && event.is_ok() {
            self.first_event = true;
            self.record("first_parsed_event");
        }
        let content = match event {
            Ok(ResponseEvent::OutputTextDelta(text)) => !text.is_empty(),
            Ok(ResponseEvent::ReasoningContentDelta { delta, .. })
            | Ok(ResponseEvent::ReasoningSummaryDelta { delta, .. })
            | Ok(ResponseEvent::ToolCallInputDelta { delta, .. }) => !delta.is_empty(),
            _ => false,
        };
        if content && !self.first_content {
            self.first_content = true;
            self.record("first_content_delta");
        }
        if matches!(event, Ok(ResponseEvent::Completed { .. }) | Err(_)) && !self.finished {
            self.finished = true;
            self.record(if event.is_ok() { "completed" } else { "failed" });
        }
    }

    fn record(&self, stage: &'static str) {
        tracing::info!(request_trace = %self.id, thread_id = self.thread_id,
            stage, elapsed_ms = self.started.elapsed().as_millis(),
            content_observed = self.first_content, "[model-timing] stage");
    }
}

impl Drop for ModelRequestTiming {
    fn drop(&mut self) {
        if !self.finished {
            self.record("ended_without_completion");
        }
    }
}
