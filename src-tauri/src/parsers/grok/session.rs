use std::fs;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde_json::Value;

#[derive(Default)]
pub(super) struct SummaryMeta {
    pub(super) cwd: Option<String>,
    pub(super) title: Option<String>,
    pub(super) model: Option<String>,
    pub(super) git_branch: Option<String>,
    pub(super) created_at: Option<DateTime<Utc>>,
    pub(super) updated_at: Option<DateTime<Utc>>,
}

pub(super) fn read_summary_json(session_dir: &Path) -> SummaryMeta {
    let Ok(raw) = fs::read_to_string(session_dir.join("summary.json")) else {
        return SummaryMeta::default();
    };
    let Ok(value) = serde_json::from_str::<Value>(&raw) else {
        return SummaryMeta::default();
    };
    let non_empty = |text: &str| {
        let trimmed = text.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    };
    SummaryMeta {
        cwd: value
            .pointer("/info/cwd")
            .and_then(Value::as_str)
            .and_then(non_empty),
        title: value
            .get("generated_title")
            .and_then(Value::as_str)
            .and_then(non_empty)
            .or_else(|| {
                value
                    .get("session_summary")
                    .and_then(Value::as_str)
                    .and_then(non_empty)
            }),
        model: value
            .get("current_model_id")
            .and_then(Value::as_str)
            .and_then(non_empty),
        git_branch: value
            .get("head_branch")
            .and_then(Value::as_str)
            .and_then(non_empty),
        created_at: value
            .get("created_at")
            .and_then(Value::as_str)
            .and_then(parse_rfc3339),
        updated_at: value
            .get("updated_at")
            .and_then(Value::as_str)
            .and_then(parse_rfc3339),
    }
}

fn parse_rfc3339(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| date.with_timezone(&Utc))
}
