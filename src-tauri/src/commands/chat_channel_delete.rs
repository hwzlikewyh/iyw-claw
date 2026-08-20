use crate::app_error::AppCommandError;
use crate::chat_channel::manager::ChatChannelManager;
use crate::db::service::{chat_channel_service, chat_channel_target_service};
use crate::db::AppDatabase;

pub async fn delete_chat_channel_core(
    db: &AppDatabase,
    manager: &ChatChannelManager,
    id: i32,
) -> Result<(), AppCommandError> {
    let _guard = crate::chat_channel::operation_lock::lock_channel(id).await;
    let current = chat_channel_service::get_by_id(&db.conn, id)
        .await
        .map_err(AppCommandError::from)?;
    let desired_enabled = current.as_ref().is_some_and(|channel| channel.enabled);
    let reconcile_wecom_skill = current
        .as_ref()
        .is_some_and(|channel| channel.channel_type == "wecom");
    let token_backup =
        crate::keyring_store::try_get_channel_token(id).map_err(secret_read_error)?;
    let target_backup = chat_channel_target_service::take_secure_targets(&db.conn, id)
        .await
        .map_err(AppCommandError::from)?;

    if let Err(error) = crate::keyring_store::delete_channel_token(id) {
        let failures = restore_targets(&target_backup).err().into_iter().collect();
        return Err(with_compensation_failures(
            id,
            secret_delete_error(error),
            failures,
        ));
    }

    if let Err(error) = manager.remove_channel(id).await {
        let failures = restore_credentials(id, token_backup.as_deref(), &target_backup);
        return Err(with_compensation_failures(
            id,
            AppCommandError::from(error),
            failures,
        ));
    }
    if let Err(error) = chat_channel_service::delete(&db.conn, id).await {
        let mut failures = restore_credentials(id, token_backup.as_deref(), &target_backup);
        if let Err(restore_error) = restore_runtime(db, manager, id, desired_enabled).await {
            failures.push(format!("runtime restore failed: {restore_error}"));
        }
        return Err(with_compensation_failures(
            id,
            AppCommandError::from(error),
            failures,
        ));
    }
    if reconcile_wecom_skill {
        super::chat_channel::reconcile_wecom_unified_best_effort(db, "delete", Some(id)).await;
    }
    Ok(())
}

async fn restore_runtime(
    db: &AppDatabase,
    manager: &ChatChannelManager,
    channel_id: i32,
    desired_enabled: bool,
) -> Result<(), AppCommandError> {
    crate::chat_channel::reconcile::reconcile_channel_unlocked(
        &db.conn,
        manager,
        channel_id,
        desired_enabled,
        "delete_rollback",
    )
    .await
    .map(|_| ())
}

fn restore_credentials(
    channel_id: i32,
    token: Option<&str>,
    targets: &[(String, String)],
) -> Vec<String> {
    let mut failures = Vec::new();
    if let Err(error) = restore_token(channel_id, token) {
        failures.push(format!("credential restore failed: {error}"));
    }
    if let Err(error) = restore_targets(targets) {
        failures.push(format!("target restore failed: {error}"));
    }
    failures
}

fn restore_token(channel_id: i32, token: Option<&str>) -> Result<(), String> {
    let Some(token) = token else {
        return Ok(());
    };
    crate::keyring_store::set_channel_token(channel_id, token)
}

fn restore_targets(targets: &[(String, String)]) -> Result<(), String> {
    chat_channel_target_service::restore_secure_targets(targets).map_err(|error| error.to_string())
}

fn with_compensation_failures(
    channel_id: i32,
    mut primary: AppCommandError,
    failures: Vec<String>,
) -> AppCommandError {
    if failures.is_empty() {
        return primary;
    }
    let failures = failures.join("; ");
    tracing::error!(
        channel_id,
        compensation_error = %failures,
        "channel delete compensation failed"
    );
    let detail = primary
        .detail
        .take()
        .unwrap_or_else(|| primary.message.clone());
    primary.with_detail(format!("{detail}; compensation failed: {failures}"))
}

fn secret_read_error(error: String) -> AppCommandError {
    AppCommandError::io_error("Failed to read channel credential").with_detail(error)
}

fn secret_delete_error(error: String) -> AppCommandError {
    AppCommandError::io_error("Failed to delete channel credential").with_detail(error)
}
