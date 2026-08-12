use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::db::entities::chat_channel_tool_request;
use crate::db::service::chat_channel_tool_request_service as requests;
use crate::db::AppDatabase;

pub enum BeginOutcome {
    Started(chat_channel_tool_request::Model),
    Cached(Value),
    Processing,
}

pub async fn begin(
    db: &AppDatabase,
    caller_scope: &str,
    operation: &str,
    request_id: &str,
    input: &Value,
) -> Result<BeginOutcome, String> {
    require_request_id(request_id)?;
    let digest = digest(input)?;
    if let Some(existing) = requests::find(&db.conn, caller_scope, operation, request_id)
        .await
        .map_err(|_| "IDEMPOTENCY_UNAVAILABLE".to_string())?
    {
        return existing_outcome(existing, &digest);
    }
    match requests::begin(&db.conn, caller_scope, operation, request_id, &digest).await {
        Ok(model) => Ok(BeginOutcome::Started(model)),
        Err(_) => {
            let existing = requests::find(&db.conn, caller_scope, operation, request_id)
                .await
                .map_err(|_| "IDEMPOTENCY_UNAVAILABLE".to_string())?
                .ok_or_else(|| "IDEMPOTENCY_UNAVAILABLE".to_string())?;
            existing_outcome(existing, &digest)
        }
    }
}

pub async fn finish(
    db: &AppDatabase,
    model: chat_channel_tool_request::Model,
    result: &Value,
) -> Result<(), String> {
    let status = if result.get("error").is_some() {
        "failed"
    } else {
        "completed"
    };
    let json = serde_json::to_string(result).map_err(|_| "IDEMPOTENCY_UNAVAILABLE".to_string())?;
    requests::finish(&db.conn, model, status, json)
        .await
        .map_err(|_| "IDEMPOTENCY_UNAVAILABLE".to_string())
}

pub async fn cancel(
    db: &AppDatabase,
    caller_scope: &str,
    operation: &str,
    request_id: &str,
) -> Result<bool, String> {
    requests::cancel(
        &db.conn,
        caller_scope,
        operation,
        request_id,
        serde_json::json!({ "error": "REQUEST_CANCELED" }).to_string(),
    )
    .await
    .map_err(|_| "IDEMPOTENCY_UNAVAILABLE".to_string())
}

fn existing_outcome(
    existing: chat_channel_tool_request::Model,
    digest: &str,
) -> Result<BeginOutcome, String> {
    if existing.input_digest != digest {
        return Err("IDEMPOTENCY_CONFLICT".to_string());
    }
    match existing.result_json {
        Some(json) => serde_json::from_str(&json)
            .map(BeginOutcome::Cached)
            .map_err(|_| "IDEMPOTENCY_UNAVAILABLE".to_string()),
        None => Ok(BeginOutcome::Processing),
    }
}

fn digest(input: &Value) -> Result<String, String> {
    let secret = crate::keyring_store::get_or_create_channel_target_secret()
        .map_err(|_| "IDEMPOTENCY_UNAVAILABLE".to_string())?;
    let bytes = serde_json::to_vec(input).map_err(|_| "INVALID_INPUT".to_string())?;
    let mut hasher = Sha256::new();
    hasher.update(secret.as_bytes());
    hasher.update([0]);
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn require_request_id(value: &str) -> Result<(), String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 128 {
        return Err("INVALID_REQUEST_ID".to_string());
    }
    Ok(())
}
