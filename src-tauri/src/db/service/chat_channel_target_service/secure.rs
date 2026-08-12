use sea_orm::DatabaseConnection;
use sha2::{Digest, Sha256};

use crate::chat_channel::types::ChannelMessageTarget;
use crate::db::error::DbError;

pub async fn take_secure_targets(
    conn: &DatabaseConnection,
    channel_id: i32,
) -> Result<Vec<(String, String)>, DbError> {
    let targets = super::list_by_channel(conn, channel_id).await?;
    let mut backup = Vec::with_capacity(targets.len());
    for target in &targets {
        let payload = crate::keyring_store::get_channel_target(&target.target_id)
            .ok_or_else(|| DbError::Migration("channel target payload unavailable".to_string()))?;
        backup.push((target.target_id.clone(), payload));
    }
    for target in targets {
        if let Err(error) = crate::keyring_store::delete_channel_target(&target.target_id) {
            let _ = restore_secure_targets(&backup);
            return Err(DbError::Migration(error));
        }
    }
    Ok(backup)
}

pub fn restore_secure_targets(targets: &[(String, String)]) -> Result<(), DbError> {
    for (target_id, payload) in targets {
        crate::keyring_store::set_channel_target(target_id, payload).map_err(DbError::Migration)?;
    }
    Ok(())
}

pub(super) fn fingerprint(target: &ChannelMessageTarget) -> Result<String, DbError> {
    let payload = target_identity(target)?;
    let secret =
        crate::keyring_store::get_or_create_channel_target_secret().map_err(DbError::Migration)?;
    let mut hasher = Sha256::new();
    hasher.update(secret.as_bytes());
    hasher.update([0]);
    hasher.update(payload.as_bytes());
    Ok(format!("{:x}", hasher.finalize()))
}

fn target_identity(target: &ChannelMessageTarget) -> Result<String, DbError> {
    serde_json::to_string(&serde_json::json!({
        "channel_id": target.channel_id,
        "chat_id": target.chat_id,
        "thread_key": target.thread_key,
        "thread_kind": target.thread_kind,
    }))
    .map_err(|error| DbError::Validation(format!("invalid channel target identity: {error}")))
}
