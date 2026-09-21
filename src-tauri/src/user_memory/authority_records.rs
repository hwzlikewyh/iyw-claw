use super::authority_types::AuthorityData;
use super::index_types::{IndexItem, IndexRelation};
use super::{authority_sql as sql, UserMemoryService};
use crate::app_error::AppCommandError;
use sea_orm::ConnectionTrait;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct RecordPayload {
    pub item: IndexItem,
    pub state: String,
    pub relations: Vec<IndexRelation>,
    pub source: Option<serde_json::Value>,
}

pub(super) async fn persist<C: ConnectionTrait>(
    db: &C,
    service: &UserMemoryService,
    change: (&AuthorityData, Option<(&str, &str)>),
) -> Result<(), AppCommandError> {
    let key = service.authority_key()?;
    let (data, rename) = change;
    let payloads = super::authority_inventory::records(service, data)?;
    if let Some((old, new)) = rename {
        redirect_identity(db, &key, (old, new)).await?;
    }
    let mut seen = BTreeSet::new();
    for payload in payloads {
        let stable = identity(db, &key, &payload.item.id)
            .await?
            .unwrap_or_else(|| payload.item.id.clone());
        if !seen.insert(stable.clone()) {
            return Err(super::helpers::conflict(
                "Correction collides with an existing memory identity",
            ));
        }
        save_record(db, (&key, &stable), payload).await?;
    }
    for row in sql::rows(
        db,
        "SELECT record_id, revision FROM memory_record WHERE root_key=? AND state!='retracted'",
        vec![key.clone().into()],
    )
    .await?
    {
        let id: String = sql::field(&row, "record_id")?;
        if seen.contains(&id) {
            continue;
        }
        let revision: i64 = sql::field(&row, "revision")?;
        let previous = sql::rows(
            db,
            "SELECT item_json FROM memory_revision WHERE root_key=? AND record_id=? AND revision=?",
            vec![key.clone().into(), id.clone().into(), revision.into()],
        )
        .await?;
        let mut payload: RecordPayload =
            serde_json::from_str(&sql::field::<String>(&previous[0], "item_json")?)
                .map_err(|_| AppCommandError::configuration_invalid("Invalid memory revision"))?;
        payload.state = "retracted".into();
        save_record(db, (&key, &id), payload).await?;
    }
    Ok(())
}

pub(super) async fn verify<C: ConnectionTrait>(
    db: &C,
    service: &UserMemoryService,
    data: &AuthorityData,
) -> Result<(), AppCommandError> {
    let key = service.authority_key()?;
    let payloads = super::authority_inventory::records(service, data)?;
    let rows = sql::rows(db, "SELECT record_id,source_id,content_digest FROM memory_record WHERE root_key=? AND state!='retracted'", vec![key.clone().into()]).await?;
    if rows.len() != payloads.len() {
        return Err(super::helpers::conflict(
            "Memory shadow import count mismatch",
        ));
    }
    for payload in payloads {
        let digest = super::helpers::hash_parts(&[super::authority::encode(&payload)?.as_bytes()]);
        let matched = rows.iter().any(|row| {
            sql::field::<String>(row, "source_id").ok().as_deref() == Some(&payload.item.id)
                && sql::field::<String>(row, "content_digest").ok().as_deref() == Some(&digest)
        });
        if !matched {
            return Err(super::helpers::conflict(
                "Memory shadow import record mismatch",
            ));
        }
    }
    Ok(())
}

pub(super) async fn identity<C: ConnectionTrait>(
    db: &C,
    key: &str,
    source: &str,
) -> Result<Option<String>, AppCommandError> {
    sql::rows(
        db,
        "SELECT record_id FROM memory_identity WHERE root_key=? AND source_id=?",
        vec![key.into(), source.into()],
    )
    .await?
    .into_iter()
    .next()
    .map(|row| sql::field(&row, "record_id"))
    .transpose()
}

async fn redirect_identity<C: ConnectionTrait>(
    db: &C,
    key: &str,
    rename: (&str, &str),
) -> Result<(), AppCommandError> {
    let (old, new) = rename;
    if old == new {
        return Ok(());
    }
    let stable = identity(db, key, old)
        .await?
        .ok_or_else(|| super::helpers::conflict("Original memory identity was not found"))?;
    if identity(db, key, new)
        .await?
        .is_some_and(|other| other != stable)
    {
        return Err(super::helpers::conflict(
            "Corrected text already belongs to another memory",
        ));
    }
    sql::execute(db,"INSERT INTO memory_identity(root_key,source_id,record_id) VALUES(?,?,?) ON CONFLICT(root_key,source_id) DO NOTHING",vec![key.into(),new.into(),stable.into()]).await?;
    Ok(())
}

async fn save_record<C: ConnectionTrait>(
    db: &C,
    identity: (&str, &str),
    payload: RecordPayload,
) -> Result<(), AppCommandError> {
    let (key, id) = identity;
    let json = super::authority::encode(&payload)?;
    let digest = super::helpers::hash_parts(&[json.as_bytes()]);
    let current = sql::rows(
        db,
        "SELECT revision,content_digest FROM memory_record WHERE root_key=? AND record_id=?",
        vec![key.into(), id.into()],
    )
    .await?;
    if current.first().is_some_and(|row| {
        sql::field::<String>(row, "content_digest").ok().as_deref() == Some(&digest)
    }) {
        return Ok(());
    }
    let revision = current
        .first()
        .map(|row| sql::field::<i64>(row, "revision"))
        .transpose()?
        .unwrap_or(0)
        + 1;
    sql::execute(db,"INSERT INTO memory_record(root_key,record_id,source_id,revision,kind,content_digest,state) VALUES(?,?,?,?,?,?,?) ON CONFLICT(root_key,record_id) DO UPDATE SET source_id=excluded.source_id,revision=excluded.revision,kind=excluded.kind,content_digest=excluded.content_digest,state=excluded.state",
        vec![key.into(),id.into(),payload.item.id.clone().into(),revision.into(),payload.item.kind.clone().into(),digest.into(),payload.state.clone().into()]).await?;
    sql::execute(db,"INSERT INTO memory_identity(root_key,source_id,record_id) VALUES(?,?,?) ON CONFLICT(root_key,source_id) DO NOTHING",vec![key.into(),payload.item.id.clone().into(),id.into()]).await?;
    sql::execute(db,"INSERT INTO memory_revision(root_key,record_id,revision,source_id,content_digest,state,item_json,reason,recorded_at) VALUES(?,?,?,?,?,?,?,?,?)",
        vec![key.into(),id.into(),revision.into(),payload.item.id.into(),payload.item.content_digest.into(),payload.state.into(),json.into(),if revision==1 {"import_or_create"}else{"memory_change"}.into(),chrono::Utc::now().to_rfc3339().into()]).await?;
    Ok(())
}
