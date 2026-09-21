use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use chrono::{Local, Timelike, Utc};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder};
use tokio::task::JoinHandle;

use super::error::ChatChannelError;
use super::i18n::Lang;
use super::manager::ChatChannelManager;
use super::message_formatter::{self, DailyReportData};
use crate::db::entities::conversation;
use crate::db::service::{
    app_metadata_service, chat_channel_message_log_service, chat_channel_service,
};

const MESSAGE_LANGUAGE_KEY: &str = "chat_message_language";
const RETRY_DELAY: Duration = Duration::from_secs(5 * 60);

pub fn spawn_daily_report_scheduler(
    manager: ChatChannelManager,
    db_conn: DatabaseConnection,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut retry_after = HashMap::<i32, Instant>::new();
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let now = Local::now();
            let channels = match chat_channel_service::list_enabled(&db_conn).await {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("[ChatChannel] scheduler: failed to list channels: {e}");
                    continue;
                }
            };
            let enabled: HashSet<i32> = channels.iter().map(|channel| channel.id).collect();
            retry_after.retain(|channel_id, _| enabled.contains(channel_id));
            for ch in &channels {
                if due(&db_conn, ch, now, &retry_after).await {
                    dispatch_report(&manager, &db_conn, ch, now, &mut retry_after).await;
                }
            }
        }
    })
}

async fn due(
    db: &DatabaseConnection,
    channel: &crate::db::entities::chat_channel::Model,
    now: chrono::DateTime<Local>,
    retry_after: &HashMap<i32, Instant>,
) -> bool {
    if !channel.daily_report_enabled
        || retry_after
            .get(&channel.id)
            .is_some_and(|retry| *retry > Instant::now())
        || !time_reached(channel.daily_report_time.as_deref().unwrap_or("18:00"), now)
    {
        return false;
    }
    let key = report_key(channel.id);
    let today = now.format("%Y-%m-%d").to_string();
    match app_metadata_service::get_value(db, &key).await {
        Ok(last) => last.as_deref() != Some(today.as_str()),
        Err(error) => {
            tracing::warn!(channel_id = channel.id, error = %error, "[ChatChannel] daily report state read failed");
            false
        }
    }
}

async fn dispatch_report(
    manager: &ChatChannelManager,
    db: &DatabaseConnection,
    channel: &crate::db::entities::chat_channel::Model,
    now: chrono::DateTime<Local>,
    retry_after: &mut HashMap<i32, Instant>,
) {
    let message = message_formatter::format_daily_report(
        &generate_daily_report(db).await,
        load_lang(db).await,
    );
    let target = super::target_registry::resolve_default(db, channel.id)
        .await
        .ok()
        .flatten();
    let target_id = target.as_ref().map(|(row, _)| row.target_id.clone());
    let result = match target.as_ref() {
        Some((_, target)) => manager.send_to_target(target, &message).await,
        None => manager.send_to_channel(channel.id, &message).await,
    };
    let (status, detail, accepted) = delivery_result(&result);
    let _ = chat_channel_message_log_service::create_log_for_target(
        db,
        channel.id,
        "outbound",
        "daily_report",
        &message.to_plain_text(),
        status,
        detail,
        None,
        result.ok().map(|id| id.0),
        target_id,
    )
    .await;
    if accepted && mark_dispatched(db, channel.id, now).await {
        retry_after.remove(&channel.id);
    } else {
        retry_after.insert(channel.id, Instant::now() + RETRY_DELAY);
    }
}

fn delivery_result(
    result: &Result<super::types::SentMessageId, ChatChannelError>,
) -> (&'static str, Option<String>, bool) {
    match result {
        Ok(_) => ("sent", None, true),
        Err(ChatChannelError::DeliveryDeferred(_)) => {
            ("queued", Some("WAITING_CONTEXT".to_string()), true)
        }
        Err(_) => ("failed", Some("CHANNEL_SEND_FAILED".to_string()), false),
    }
}

async fn mark_dispatched(
    db: &DatabaseConnection,
    channel_id: i32,
    now: chrono::DateTime<Local>,
) -> bool {
    match app_metadata_service::upsert_value(
        db,
        &report_key(channel_id),
        &now.format("%Y-%m-%d").to_string(),
    )
    .await
    {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(channel_id, error = %error, "[ChatChannel] daily report state write failed");
            false
        }
    }
}

fn time_reached(value: &str, now: chrono::DateTime<Local>) -> bool {
    let Some((hour, minute)) = value.split_once(':') else {
        return false;
    };
    let Ok(hour) = hour.parse::<u32>() else {
        return false;
    };
    let Ok(minute) = minute.parse::<u32>() else {
        return false;
    };
    (now.hour(), now.minute()) >= (hour.min(23), minute.min(59))
}

fn report_key(channel_id: i32) -> String {
    format!("chat_channel_daily_report_last_dispatch:{channel_id}")
}

async fn load_lang(db: &DatabaseConnection) -> Lang {
    app_metadata_service::get_value(db, MESSAGE_LANGUAGE_KEY)
        .await
        .ok()
        .flatten()
        .map(|v| Lang::from_str_lossy(&v))
        .unwrap_or_default()
}

async fn generate_daily_report(db: &DatabaseConnection) -> DailyReportData {
    let now = Utc::now();
    let today_start = now.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc();

    let rows = conversation::Entity::find()
        .filter(conversation::Column::DeletedAt.is_null())
        .filter(conversation::Column::CreatedAt.gte(today_start))
        .order_by_desc(conversation::Column::CreatedAt)
        .all(db)
        .await
        .unwrap_or_default();

    let mut by_agent: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    let mut folder_ids: HashSet<i32> = HashSet::new();
    let mut activities: Vec<String> = Vec::new();

    for conv in &rows {
        *by_agent.entry(conv.agent_type.clone()).or_insert(0) += 1;
        folder_ids.insert(conv.folder_id);
        if let Some(title) = &conv.title {
            if activities.len() < 10 {
                activities.push(title.clone());
            }
        }
    }

    // Resolve folder names
    let mut project_names: Vec<String> = Vec::new();
    for fid in &folder_ids {
        if let Ok(Some(folder)) = crate::db::entities::folder::Entity::find_by_id(*fid)
            .one(db)
            .await
        {
            project_names.push(folder.name);
        }
    }

    let conversations_by_agent: Vec<(String, u32)> = by_agent.into_iter().collect();

    DailyReportData {
        date: now.format("%Y-%m-%d").to_string(),
        total_conversations: rows.len() as u32,
        conversations_by_agent,
        projects_involved: project_names,
        key_activities: activities,
    }
}
