use std::sync::OnceLock;

use chrono::NaiveDate;
use sea_orm::{DatabaseConnection, TransactionTrait};
use serde::de::DeserializeOwned;
use tokio::sync::Mutex;

use crate::db::error::DbError;
use crate::db::service::app_metadata_service;
use crate::models::UsageDashboardStats;

pub use crate::models::SessionUsageSnapshot;

const DASHBOARD_KEY: &str = "usage.dashboard.v1";
const SESSION_KEY_PREFIX: &str = "usage.session.v1.";

fn cache_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn session_key(conversation_id: i32) -> String {
    format!("{SESSION_KEY_PREFIX}{conversation_id}")
}

fn decode<T: DeserializeOwned>(raw: Option<String>, label: &str) -> Result<Option<T>, DbError> {
    raw.map(|value| {
        serde_json::from_str(&value)
            .map_err(|error| DbError::Validation(format!("invalid {label}: {error}")))
    })
    .transpose()
}

fn encode<T: serde::Serialize>(value: &T, label: &str) -> Result<String, DbError> {
    serde_json::to_string(value)
        .map_err(|error| DbError::Validation(format!("failed to encode {label}: {error}")))
}

fn validate_snapshot(snapshot: &SessionUsageSnapshot) -> Result<(), DbError> {
    NaiveDate::parse_from_str(&snapshot.date, "%Y-%m-%d")
        .map_err(|_| DbError::Validation("usage snapshot date must be YYYY-MM-DD".into()))?;
    if snapshot.model.trim().is_empty() {
        return Err(DbError::Validation(
            "usage snapshot model must not be empty".into(),
        ));
    }
    Ok(())
}

pub async fn get_dashboard(conn: &DatabaseConnection) -> Result<UsageDashboardStats, DbError> {
    let raw = app_metadata_service::get_value(conn, DASHBOARD_KEY).await?;
    Ok(decode(raw, "usage dashboard")?.unwrap_or_default())
}

pub async fn upsert_session_snapshot(
    conn: &DatabaseConnection,
    snapshot: SessionUsageSnapshot,
) -> Result<(), DbError> {
    validate_snapshot(&snapshot)?;
    let _guard = cache_lock().lock().await;
    let txn = conn.begin().await?;
    let key = session_key(snapshot.conversation_id);
    let previous = decode(
        app_metadata_service::get_value_conn(&txn, &key).await?,
        "usage session snapshot",
    )?;
    let mut dashboard: UsageDashboardStats = decode(
        app_metadata_service::get_value_conn(&txn, DASHBOARD_KEY).await?,
        "usage dashboard",
    )?
    .unwrap_or_default();
    dashboard.replace_session(previous.as_ref(), &snapshot);
    app_metadata_service::upsert_value(&txn, &key, &encode(&snapshot, "usage session snapshot")?)
        .await?;
    app_metadata_service::upsert_value(
        &txn,
        DASHBOARD_KEY,
        &encode(&dashboard, "usage dashboard")?,
    )
    .await?;
    txn.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::db::test_helpers::fresh_in_memory_db;
    use crate::models::TurnUsage;

    use super::{get_dashboard, upsert_session_snapshot, SessionUsageSnapshot};

    fn snapshot(
        conversation_id: i32,
        date: &str,
        model: &str,
        input: u64,
        output: u64,
        cache_read: u64,
        cache_write: u64,
    ) -> SessionUsageSnapshot {
        SessionUsageSnapshot {
            conversation_id,
            date: date.to_string(),
            model: model.to_string(),
            usage: TurnUsage {
                input_tokens: input,
                output_tokens: output,
                cache_read_input_tokens: cache_read,
                cache_creation_input_tokens: cache_write,
            },
        }
    }

    #[tokio::test]
    async fn empty_dashboard_is_returned_without_session_scanning() {
        let db = fresh_in_memory_db().await;

        let dashboard = get_dashboard(&db.conn).await.expect("dashboard");

        assert_eq!(dashboard.total_tokens, 0);
        assert_eq!(dashboard.session_count, 0);
        assert!(dashboard.model_rows.is_empty());
        assert!(dashboard.daily_rows.is_empty());
    }

    #[tokio::test]
    async fn first_snapshot_is_added_to_cached_dashboard() {
        let db = fresh_in_memory_db().await;

        upsert_session_snapshot(
            &db.conn,
            snapshot(1, "2026-07-10", "gpt-5.4", 100, 20, 50, 10),
        )
        .await
        .expect("upsert");

        let dashboard = get_dashboard(&db.conn).await.expect("dashboard");
        assert_eq!(dashboard.session_count, 1);
        assert_eq!(dashboard.total_tokens, 180);
        assert_eq!(dashboard.total.input, 100);
        assert_eq!(dashboard.total.output, 20);
        assert_eq!(dashboard.total.cache_read, 50);
        assert_eq!(dashboard.total.cache_write, 10);
        assert_eq!(dashboard.model_rows.len(), 1);
        assert_eq!(dashboard.model_rows[0].model, "gpt-5.4");
        assert_eq!(dashboard.model_rows[0].sessions, 1);
        assert_eq!(dashboard.daily_rows.len(), 1);
        assert_eq!(dashboard.daily_rows[0].date, "2026-07-10");
    }

    #[tokio::test]
    async fn later_turn_replaces_same_session_snapshot_without_double_counting() {
        let db = fresh_in_memory_db().await;
        upsert_session_snapshot(
            &db.conn,
            snapshot(7, "2026-07-10", "gpt-5.4", 100, 20, 50, 0),
        )
        .await
        .expect("first upsert");

        upsert_session_snapshot(
            &db.conn,
            snapshot(7, "2026-07-10", "gpt-5.4", 240, 60, 80, 10),
        )
        .await
        .expect("second upsert");

        let dashboard = get_dashboard(&db.conn).await.expect("dashboard");
        assert_eq!(dashboard.session_count, 1);
        assert_eq!(dashboard.total_tokens, 390);
        assert_eq!(dashboard.total.input, 240);
        assert_eq!(dashboard.total.output, 60);
        assert_eq!(dashboard.total.cache_read, 80);
        assert_eq!(dashboard.total.cache_write, 10);
        assert_eq!(dashboard.model_rows[0].sessions, 1);
        assert_eq!(dashboard.daily_rows[0].sessions, 1);
    }

    #[tokio::test]
    async fn snapshots_are_grouped_by_model_and_day() {
        let db = fresh_in_memory_db().await;
        for item in [
            snapshot(1, "2026-07-09", "gpt-5.4", 100, 20, 0, 0),
            snapshot(2, "2026-07-10", "gpt-5.4", 200, 30, 40, 0),
            snapshot(3, "2026-07-10", "claude-sonnet", 50, 10, 0, 0),
        ] {
            upsert_session_snapshot(&db.conn, item)
                .await
                .expect("upsert");
        }

        let dashboard = get_dashboard(&db.conn).await.expect("dashboard");
        assert_eq!(dashboard.session_count, 3);
        assert_eq!(dashboard.total_tokens, 450);
        assert_eq!(dashboard.model_rows.len(), 2);
        assert_eq!(dashboard.model_rows[0].model, "gpt-5.4");
        assert_eq!(dashboard.model_rows[0].sessions, 2);
        assert_eq!(dashboard.model_rows[0].total, 390);
        assert_eq!(dashboard.daily_rows.len(), 2);
        assert_eq!(dashboard.daily_rows[1].date, "2026-07-10");
        assert_eq!(dashboard.daily_rows[1].sessions, 2);
        assert_eq!(dashboard.daily_rows[1].total, 330);
    }
}
