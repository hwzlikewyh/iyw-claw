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

pub async fn list_session_snapshots(
    conn: &DatabaseConnection,
) -> Result<Vec<SessionUsageSnapshot>, DbError> {
    let values = app_metadata_service::list_values_by_key_prefix(conn, SESSION_KEY_PREFIX).await?;
    Ok(values
        .into_iter()
        .filter_map(|value| match serde_json::from_str(&value) {
            Ok(snapshot) => Some(snapshot),
            Err(error) => {
                tracing::warn!("ignoring invalid usage session snapshot: {error}");
                None
            }
        })
        .collect())
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

