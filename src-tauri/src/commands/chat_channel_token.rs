use tokio::sync::Mutex;

use crate::app_error::AppCommandError;
use crate::chat_channel::manager::ChatChannelManager;
use crate::db::service::chat_channel_service;
use crate::db::AppDatabase;

static TOKEN_UPDATE_LOCK: Mutex<()> = Mutex::const_new(());

pub async fn save_chat_channel_token_core(
    db: &AppDatabase,
    manager: &ChatChannelManager,
    channel_id: i32,
    token: &str,
) -> Result<(), AppCommandError> {
    save_chat_channel_token_with_patch_core(db, manager, channel_id, token, None).await
}

pub async fn save_chat_channel_token_with_patch_core(
    db: &AppDatabase,
    manager: &ChatChannelManager,
    channel_id: i32,
    token: &str,
    config_patch_json: Option<String>,
) -> Result<(), AppCommandError> {
    let _channel_guard = crate::chat_channel::operation_lock::lock_channel(channel_id).await;
    let _guard = TOKEN_UPDATE_LOCK.lock().await;
    let current = chat_channel_service::get_by_id(&db.conn, channel_id)
        .await
        .map_err(AppCommandError::from)?
        .ok_or_else(|| {
            AppCommandError::not_found(format!("Chat channel {channel_id} not found"))
        })?;
    let token_backup =
        crate::keyring_store::try_get_channel_token(channel_id).map_err(secret_read_error)?;
    crate::keyring_store::set_channel_token(channel_id, token).map_err(secret_write_error)?;
    if let Some(patch) = config_patch_json {
        if current.channel_type != "wecom_agent" {
            restore_token(channel_id, token_backup.as_deref()).map_err(secret_write_error)?;
            return Err(AppCommandError::invalid_input(
                "Credential configuration patch is only supported for WeCom Agent",
            ));
        }
        if let Err(error) = update_wecom_config(db, manager, channel_id, patch).await {
            restore_after_update_error(channel_id, token_backup.as_deref(), &error)?;
            return Err(error);
        }
    } else if current.enabled {
        let _ = super::chat_channel::reconcile_channel_or_log_unlocked(
            db,
            manager,
            channel_id,
            true,
            "credential",
        )
        .await;
    }
    Ok(())
}

pub async fn delete_chat_channel_token_core(
    db: &AppDatabase,
    manager: &ChatChannelManager,
    channel_id: i32,
) -> Result<(), AppCommandError> {
    let _channel_guard = crate::chat_channel::operation_lock::lock_channel(channel_id).await;
    let _guard = TOKEN_UPDATE_LOCK.lock().await;
    let current = chat_channel_service::get_by_id(&db.conn, channel_id)
        .await
        .map_err(AppCommandError::from)?
        .ok_or_else(|| {
            AppCommandError::not_found(format!("Chat channel {channel_id} not found"))
        })?;
    let token_backup =
        crate::keyring_store::try_get_channel_token(channel_id).map_err(secret_read_error)?;
    crate::keyring_store::delete_channel_token(channel_id).map_err(secret_delete_error)?;
    let result = crate::chat_channel::reconcile::reconcile_channel_unlocked(
        &db.conn,
        manager,
        channel_id,
        false,
        "credential_delete",
    )
    .await;
    if let Err(error) = result {
        rollback_token_delete(db, manager, &current, token_backup.as_deref(), &error).await?;
        return Err(error);
    }
    Ok(())
}

async fn update_wecom_config(
    db: &AppDatabase,
    manager: &ChatChannelManager,
    channel_id: i32,
    patch: String,
) -> Result<(), AppCommandError> {
    super::chat_channel::update_chat_channel_core_unlocked(
        db,
        manager,
        channel_id,
        None,
        None,
        Some(patch),
        None,
        None,
        None,
    )
    .await
    .map(|_| ())
}

fn restore_after_update_error(
    channel_id: i32,
    backup: Option<&str>,
    update_error: &AppCommandError,
) -> Result<(), AppCommandError> {
    restore_token(channel_id, backup).map_err(|rollback_error| {
        tracing::error!(
            channel_id,
            error = %rollback_error,
            "channel credential rollback failed after configuration update failure"
        );
        AppCommandError::io_error("Failed to save channel configuration safely").with_detail(
            format!("{update_error}; credential rollback failed: {rollback_error}"),
        )
    })
}

fn restore_token(channel_id: i32, backup: Option<&str>) -> Result<(), String> {
    match backup {
        Some(token) => crate::keyring_store::set_channel_token(channel_id, token),
        None => crate::keyring_store::delete_channel_token(channel_id),
    }
}

fn secret_read_error(error: String) -> AppCommandError {
    AppCommandError::io_error("Failed to read existing channel credential").with_detail(error)
}

fn secret_write_error(error: String) -> AppCommandError {
    AppCommandError::io_error("Failed to save channel credential").with_detail(error)
}

fn secret_delete_error(error: String) -> AppCommandError {
    AppCommandError::io_error("Failed to delete channel credential").with_detail(error)
}

async fn rollback_token_delete(
    db: &AppDatabase,
    manager: &ChatChannelManager,
    current: &crate::db::entities::chat_channel::Model,
    token_backup: Option<&str>,
    delete_error: &AppCommandError,
) -> Result<(), AppCommandError> {
    restore_token(current.id, token_backup).map_err(|rollback_error| {
        AppCommandError::io_error("Failed to delete channel credential safely").with_detail(
            format!("{delete_error}; credential rollback failed: {rollback_error}"),
        )
    })?;
    if current.enabled {
        let outcome = crate::chat_channel::reconcile::reconcile_channel_unlocked(
            &db.conn,
            manager,
            current.id,
            true,
            "credential_rollback",
        )
        .await?;
        if let Some(runtime_error) = outcome.error {
            return Err(
                AppCommandError::io_error("Failed to delete channel credential safely")
                    .with_detail(format!(
                        "{delete_error}; runtime rollback failed: {runtime_error}"
                    )),
            );
        }
    }
    Ok(())
}
