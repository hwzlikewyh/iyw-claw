use std::time::Duration;

use sea_orm::{DatabaseConnection, EntityTrait};

use crate::chat_channel::{
    error::ChatChannelError, manager::ChatChannelManager, types::ChannelMessageTarget,
};
use crate::db::entities::{automation, automation_run, chat_channel};
use crate::db::service::{
    automation_notification_service as delivery, chat_channel_message_log_service,
};
use crate::web::event_bridge::{
    emit_event, AutomationChange, EventEmitter, AUTOMATION_CHANGED_EVENT,
};

const DELIVERY_POLL_SECONDS: u64 = 5;
const DELIVERY_TIMEOUT_SECONDS: u64 = 60;

pub fn spawn(manager: ChatChannelManager, db: DatabaseConnection, emitter: EventEmitter) {
    tokio::spawn(async move {
        let cadence = Duration::from_secs(DELIVERY_POLL_SECONDS);
        let mut tick = tokio::time::interval_at(tokio::time::Instant::now() + cadence, cadence);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut failed = false;
        loop {
            tick.tick().await;
            // 与执行引擎共享所有权，避免多个应用进程同时处理同一数据库。
            if super::engine().is_none() {
                continue;
            }
            match dispatch_pending(&manager, &db, &emitter).await {
                Err(error) => {
                    if !failed {
                        tracing::warn!(error = %error, "[automation] completion notification processing failed");
                    }
                    failed = true;
                }
                Ok(()) if failed => {
                    tracing::info!("[automation] completion notification processing recovered");
                    failed = false;
                }
                Ok(()) => {}
            }
        }
    });
}

async fn dispatch_pending(
    manager: &ChatChannelManager,
    db: &DatabaseConnection,
    emitter: &EventEmitter,
) -> Result<(), crate::db::error::DbError> {
    let recovered = delivery::recover_abandoned(db).await?;
    if recovered > 0 {
        tracing::warn!(
            count = recovered,
            "[automation] interrupted notification deliveries marked unknown"
        );
    }
    for run in delivery::pending(db).await? {
        if !delivery::claim(db, run.id).await? {
            continue;
        }
        let result = tokio::time::timeout(
            Duration::from_secs(DELIVERY_TIMEOUT_SECONDS),
            send(manager, db, &run),
        )
        .await;
        let outcome = match result {
            Ok(Ok(())) => ("sent", None),
            Ok(Err(ChatChannelError::DeliveryDeferred(_))) => {
                ("queued", Some("Waiting for channel context".into()))
            }
            Ok(Err(error)) => ("failed", Some(delivery_error(&error))),
            Err(_) => (
                "failed",
                Some("Delivery timed out; result unknown. Not resent to avoid duplicates.".into()),
            ),
        };
        tracing::info!(automation_id = run.automation_id, run_id = run.id, channel_id = ?run.notification_channel_id, status = outcome.0, "[automation] completion notification settled");
        delivery::finish(db, run.id, outcome).await?;
        emit_event(
            emitter,
            AUTOMATION_CHANGED_EVENT,
            AutomationChange::Upsert {
                id: run.automation_id,
            },
        );
    }
    Ok(())
}

async fn send(
    manager: &ChatChannelManager,
    db: &DatabaseConnection,
    run: &automation_run::Model,
) -> Result<(), ChatChannelError> {
    let channel_id = run.notification_channel_id.ok_or_else(|| {
        ChatChannelError::ConfigurationInvalid("notification channel missing".into())
    })?;
    let channel = chat_channel::Entity::find_by_id(channel_id)
        .one(db)
        .await
        .map_err(|error| ChatChannelError::Other(error.to_string()))?
        .ok_or(ChatChannelError::NotFound(channel_id))?;
    if !channel.enabled || !manager.is_connected(channel_id).await {
        return Err(ChatChannelError::NotConnected);
    }
    let target: ChannelMessageTarget = serde_json::from_str(
        run.notification_target_json.as_deref().unwrap_or(""),
    )
    .map_err(|_| {
        ChatChannelError::ConfigurationInvalid("notification receiving conversation missing".into())
    })?;
    if target.channel_id != channel_id {
        return Err(ChatChannelError::ConfigurationInvalid(
            "notification target mismatch".into(),
        ));
    }
    let task = automation::Entity::find_by_id(run.automation_id)
        .one(db)
        .await
        .map_err(|error| ChatChannelError::Other(error.to_string()))?;
    let message = super::notification_message::format(
        db,
        run,
        task.as_ref()
            .map(|task| task.name.as_str())
            .unwrap_or("定时任务"),
    )
    .await;
    let result = manager.send_to_target(&target, &message).await;
    let status = match &result {
        Ok(_) => "sent",
        Err(ChatChannelError::DeliveryDeferred(_)) => "queued",
        Err(_) => "failed",
    };
    let provider_id = result.as_ref().ok().map(|id| id.0.clone());
    if let Err(error) = chat_channel_message_log_service::create_log_full(
        db,
        channel_id,
        "outbound",
        "automation_complete",
        &message.to_plain_text(),
        status,
        result.as_ref().err().map(delivery_error),
        Some(format!("automation-run-{}", run.id)),
        provider_id,
    )
    .await
    {
        tracing::warn!(run_id = run.id, channel_id, error = %error, "[automation] notification message log unavailable");
    }
    result.map(|_| ())
}

fn delivery_error(error: &ChatChannelError) -> String {
    match error {
        ChatChannelError::NotConnected => "Channel is disconnected or disabled".into(),
        ChatChannelError::NotFound(_) => "Notification channel no longer exists".into(),
        ChatChannelError::ConfigurationInvalid(detail) => detail.clone(),
        _ => format!("Notification delivery failed ({})", error.category()),
    }
}
