use sea_orm::DatabaseConnection;

use crate::app_error::AppCommandError;

use super::harvest::MemoryHarvestRequest;
use super::harvest_store_sql::{agent_name, execute};

pub(super) async fn project(
    conn: &DatabaseConnection,
    request: &MemoryHarvestRequest,
) -> Result<(), AppCommandError> {
    let Ok(conversation_id) = request.conversation.parse::<i32>() else {
        return Ok(());
    };
    let intent = safe_bounded(request.user_input_ref.as_deref().unwrap_or(""), 1_000);
    if intent.is_empty() {
        return Ok(());
    }
    let result = safe_bounded(
        &super::harvest::strip_agent_lessons(request.assistant_input_ref.as_deref().unwrap_or("")),
        1_500,
    );
    let failures = safe_bounded(request.tool_outcome_ref.as_deref().unwrap_or(""), 800);
    let decisions = extract(
        &result,
        &["决定", "选择", "采用", "改为", "will use", "chose"],
    );
    let pending = extract(
        &result,
        &["待办", "未完成", "还需", "下一步", "pending", "remaining"],
    );
    let status = if super::harvest::abnormal_stop_reason(request.stop_reason.as_deref()) {
        "failed"
    } else {
        "completed"
    };
    let digest = super::helpers::hash_parts(&[
        intent.as_bytes(),
        result.as_bytes(),
        decisions.as_bytes(),
        failures.as_bytes(),
        pending.as_bytes(),
    ]);
    execute(
        conn,
        "INSERT INTO session_task_projection (conversation_id, turn_generation, agent_type, workspace_key, intent, result, decisions, failures, pending_items, status, content_digest, occurred_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(conversation_id, turn_generation) DO UPDATE SET result = excluded.result, decisions = excluded.decisions, failures = excluded.failures, pending_items = excluded.pending_items, status = excluded.status, content_digest = excluded.content_digest, occurred_at = excluded.occurred_at",
        [conversation_id.into(), (request.turn_nonce as i64).into(), agent_name(request.agent_type).into(), request.workspace_key.clone().into(), intent.clone().into(), optional(&result).into(), optional(&decisions).into(), optional(&failures).into(), optional(&pending).into(), status.into(), digest.into(), request.submitted_at.clone().into()],
    ).await?;
    let _ = execute(
        conn,
        "DELETE FROM session_task_fts WHERE conversation_id = ? AND turn_generation = ?",
        [conversation_id.into(), (request.turn_nonce as i64).into()],
    )
    .await;
    let _ = execute(conn, "INSERT INTO session_task_fts (conversation_id, turn_generation, intent, result, decisions, failures, pending_items) VALUES (?, ?, ?, ?, ?, ?, ?)", [conversation_id.into(), (request.turn_nonce as i64).into(), intent.into(), result.into(), decisions.into(), failures.into(), pending.into()]).await;
    Ok(())
}

fn optional(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
}

fn bounded(value: &str, limit: usize) -> String {
    value
        .chars()
        .take(limit)
        .collect::<String>()
        .trim()
        .to_string()
}

fn safe_bounded(value: &str, limit: usize) -> String {
    if super::helpers::contains_potential_secret(value) {
        String::new()
    } else {
        bounded(value, limit)
    }
}

fn extract(value: &str, markers: &[&str]) -> String {
    value
        .split(['。', '！', '？', '!', '?', '\n'])
        .map(str::trim)
        .filter(|sentence| {
            let lower = sentence.to_ascii_lowercase();
            markers
                .iter()
                .any(|marker| sentence.contains(marker) || lower.contains(marker))
        })
        .take(3)
        .collect::<Vec<_>>()
        .join("；")
}
