use sha2::{Digest, Sha256};
use std::collections::HashMap;

use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QuerySelect, TransactionTrait,
};

use super::query::RequestRows;
use crate::db::{entities::app_metadata, error::DbError, service::app_metadata_service};

const KEY_PREFIX: &str = "usage.request.confirmed.v2";
const READ_BATCH: usize = 100;

pub(super) fn scope(url: &str, token: &str, identity: &str) -> String {
    format!(
        "{:x}",
        Sha256::digest(
            serde_json::json!([url, token, identity])
                .to_string()
                .as_bytes()
        )
    )
}

fn key(scope: &str, request_id: &str) -> String {
    format!("{KEY_PREFIX}.{scope}.{request_id}")
}

pub(super) async fn load(
    conn: &DatabaseConnection,
    scope: &str,
    ids: &[String],
) -> Result<HashMap<String, RequestRows>, DbError> {
    let mut records = HashMap::new();
    for batch in ids.chunks(READ_BATCH) {
        let keys: Vec<_> = batch.iter().map(|id| key(scope, id)).collect();
        let values = app_metadata::Entity::find()
            .select_only()
            .columns([app_metadata::Column::Key, app_metadata::Column::Value])
            .filter(app_metadata::Column::Key.is_in(keys))
            .filter(app_metadata::Column::DeletedAt.is_null())
            .into_tuple::<(String, String)>()
            .all(conn)
            .await?;
        for (stored_key, value) in values {
            let rows = match serde_json::from_str::<RequestRows>(&value) {
                Ok(rows) => rows,
                Err(error) => {
                    tracing::warn!(%error, "[conversation-points] invalid local consumption cache");
                    continue;
                }
            };
            let Some(first) = rows.items.first() else {
                continue;
            };
            if rows.has_final_cost_rows()
                && key(scope, &first.request_id) == stored_key
                && rows
                    .items
                    .iter()
                    .all(|row| row.request_id == first.request_id)
            {
                records.insert(first.request_id.clone(), rows);
            }
        }
    }
    Ok(records)
}

pub(super) async fn save(
    conn: &DatabaseConnection,
    scope: &str,
    records: &HashMap<String, RequestRows>,
) -> Result<(), DbError> {
    if records.is_empty() {
        return Ok(());
    }
    let txn = conn.begin().await?;
    for (id, rows) in records {
        let mut cost_only = rows.clone();
        for row in &mut cost_only.items {
            row.charged_points = None;
            row.settlement_status.clear();
        }
        let value = serde_json::to_string(&cost_only)
            .map_err(|error| DbError::Validation(format!("invalid consumption cache: {error}")))?;
        app_metadata_service::upsert_value(&txn, &key(scope, id), &value).await?;
    }
    txn.commit().await?;
    tracing::debug!(
        request_count = records.len(),
        "[conversation-points] confirmed consumption cached locally"
    );
    Ok(())
}
