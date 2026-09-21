use super::{authority_sql as sql, authority_types::AuthoritySnapshot, helpers};
use crate::app_error::AppCommandError;
use sea_orm::ConnectionTrait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(super) const MAX_BUNDLE_BYTES: usize = 67_108_864;
const MAX_TABLE_ROWS: usize = 65_536;
pub(super) const TABLES: [(&str, &str); 3] = [
    (
        "memory_record",
        "record_id,source_id,revision,kind,content_digest,state",
    ),
    (
        "memory_revision",
        "record_id,revision,source_id,content_digest,state,item_json,reason,recorded_at",
    ),
    ("memory_identity", "source_id,record_id"),
];

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecoveryBundle {
    pub snapshot: AuthoritySnapshot,
    pub tables: Vec<Vec<Vec<Value>>>,
}

pub(super) async fn load<C: ConnectionTrait>(
    db: &C,
    key: &str,
) -> Result<Option<RecoveryBundle>, AppCommandError> {
    let Some(snapshot) = sql::load(db, key).await? else {
        return Ok(None);
    };
    let mut tables = Vec::new();
    let mut total = 0;
    for (table, columns) in TABLES {
        let order = if table == "memory_revision" {
            "record_id,revision"
        } else {
            columns.split(',').next().unwrap()
        };
        let query=format!("SELECT json_array({columns}) AS row FROM {table} WHERE root_key=? ORDER BY {order} LIMIT ?");
        let rows = sql::rows(
            db,
            &query,
            vec![key.into(), ((MAX_TABLE_ROWS + 1) as i64).into()],
        )
        .await?;
        if rows.len() > MAX_TABLE_ROWS {
            return Err(invalid("Recovery history exceeds the supported row limit"));
        }
        let mut values = Vec::new();
        for row in rows {
            let json: String = sql::field(&row, "row")?;
            total += json.len();
            if total > MAX_BUNDLE_BYTES {
                return Err(invalid("Recovery history exceeds the supported size limit"));
            }
            values.push(serde_json::from_str(&json).map_err(|_| invalid("Invalid recovery row"))?);
        }
        tables.push(values);
    }
    let bundle = RecoveryBundle { snapshot, tables };
    validate(&bundle)?;
    Ok(Some(bundle))
}

pub(super) fn validate(bundle: &RecoveryBundle) -> Result<(), AppCommandError> {
    super::authority_validation::validate(&bundle.snapshot.data)?;
    if !matches!(bundle.snapshot.mode.as_str(), "active" | "shadow")
        || bundle.snapshot.epoch < 1
        || super::authority_types::digest(&bundle.snapshot.data)? != bundle.snapshot.digest
        || bundle.tables.len() != TABLES.len()
    {
        return Err(invalid("Invalid recovery snapshot"));
    }
    let encoded = super::authority::encode(bundle)?;
    if encoded.len() > MAX_BUNDLE_BYTES {
        return Err(invalid("Recovery bundle is too large"));
    }
    for (index, rows) in bundle.tables.iter().enumerate() {
        let width = TABLES[index].1.split(',').count();
        if rows.len() > MAX_TABLE_ROWS
            || rows
                .iter()
                .any(|row| row.len() != width || row.iter().any(|v| !v.is_string() && !v.is_i64()))
        {
            return Err(invalid("Invalid recovery table"));
        }
    }
    super::restore_integrity::validate_history(bundle)
}

pub(super) fn fingerprint(bundle: &RecoveryBundle) -> Result<String, AppCommandError> {
    Ok(helpers::hash_parts(&[
        super::authority::encode(bundle)?.as_bytes()
    ]))
}

pub(super) fn invalid(message: &str) -> AppCommandError {
    AppCommandError::configuration_invalid(message)
}
