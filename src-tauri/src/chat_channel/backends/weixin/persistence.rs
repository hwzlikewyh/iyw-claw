use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::db::service::app_metadata_service;

use super::DatabaseConnection;

#[derive(Deserialize, Serialize)]
struct CursorState {
    auth_fingerprint: String,
    cursor: String,
}

pub(super) async fn load_cursor(
    db: &DatabaseConnection,
    channel_id: i32,
    token: &str,
) -> Option<String> {
    let raw = app_metadata_service::get_value(db, &cursor_key(channel_id))
        .await
        .ok()
        .flatten()?;
    let state: CursorState = serde_json::from_str(&raw).ok()?;
    (state.auth_fingerprint == token_fingerprint(token)).then_some(state.cursor)
}

pub(super) async fn save_cursor(
    db: &DatabaseConnection,
    channel_id: i32,
    token: &str,
    cursor: &str,
) -> Result<(), String> {
    let value = serde_json::to_string(&CursorState {
        auth_fingerprint: token_fingerprint(token),
        cursor: cursor.to_string(),
    })
    .map_err(|error| error.to_string())?;
    crate::db::retry_sqlite_maintenance("weixin_cursor", || {
        let key = cursor_key(channel_id);
        let value = &value;
        async move { app_metadata_service::upsert_value(db, &key, value).await }
    })
    .await
    .map_err(|error| error.to_string())
}

fn cursor_key(channel_id: i32) -> String {
    format!("chat_channel_weixin_cursor:{channel_id}")
}

fn token_fingerprint(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    format!("{:x}", digest)
}
