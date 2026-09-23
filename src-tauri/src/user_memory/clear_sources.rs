use std::collections::BTreeSet;

use super::authority_commit::AuthorityPurge;
use super::authority_records::RecordPayload;
use super::authority_types::AuthoritySnapshot;
use super::{authority_sql as sql, ClearUserMemoryScope, ResourceGeneration, UserMemoryService};
use crate::app_error::AppCommandError;

pub(super) struct ClearSources {
    pub purge: AuthorityPurge,
    pub backup_needles: Vec<String>,
}

pub(super) async fn collect(
    service: &UserMemoryService,
    snapshot: &AuthoritySnapshot,
    scope: ClearUserMemoryScope,
) -> Result<ClearSources, AppCommandError> {
    let (ids, mut needles) = selected_records(service, scope).await?;
    let mut contents = BTreeSet::new();
    collect_history(service, &ids, &mut contents).await?;
    for record in super::authority_inventory::records(service, &snapshot.data)? {
        if scope == ClearUserMemoryScope::All
            || !matches!(record.item.kind.as_str(), "profile" | "soul")
        {
            collect_record(record, &mut contents);
        }
    }
    for document in scope.documents() {
        if let Some(ResourceGeneration::Present { value, .. }) =
            snapshot.data.documents.get(document)
        {
            contents.insert(value.clone());
        }
    }
    contents.retain(|content| !content.trim().is_empty());
    let tombstones = contents
        .iter()
        .map(|text| super::forget::forgotten_hash(text))
        .collect();
    needles.extend(ids.iter().cloned());
    needles.extend(contents);
    let cutoff = sql::rows(
        &service.db,
        "SELECT COALESCE(MAX(id),0) AS cutoff FROM memory_harvest_outbox",
        vec![],
    )
    .await?;
    Ok(ClearSources {
        purge: AuthorityPurge {
            record_ids: ids.into_iter().collect(),
            tombstones,
            harvest_cutoff: Some(sql::field(&cutoff[0], "cutoff")?),
            pending_harvest: super::harvest_pending::pending_for_clear(service.resolved_root()?)?,
        },
        backup_needles: needles.into_iter().collect(),
    })
}

async fn selected_records(
    service: &UserMemoryService,
    scope: ClearUserMemoryScope,
) -> Result<(BTreeSet<String>, BTreeSet<String>), AppCommandError> {
    let rows = sql::rows(&service.db,
        "SELECT record_id,source_id FROM memory_record WHERE root_key=? AND (? OR kind NOT IN ('profile','soul'))",
        vec![service.authority_key()?.into(), (scope == ClearUserMemoryScope::All).into()]).await?;
    let mut ids = BTreeSet::new();
    let mut needles = BTreeSet::new();
    for row in rows {
        ids.insert(sql::field(&row, "record_id")?);
        needles.insert(sql::field(&row, "source_id")?);
    }
    Ok((ids, needles))
}

async fn collect_history(
    service: &UserMemoryService,
    ids: &BTreeSet<String>,
    contents: &mut BTreeSet<String>,
) -> Result<(), AppCommandError> {
    let key = service.authority_key()?;
    for id in ids {
        let rows = sql::rows(
            &service.db,
            "SELECT item_json FROM memory_revision WHERE root_key=? AND record_id=?",
            vec![key.clone().into(), id.clone().into()],
        )
        .await?;
        for row in rows {
            let payload = serde_json::from_str(&sql::field::<String>(&row, "item_json")?)
                .map_err(|_| AppCommandError::configuration_invalid("Invalid memory revision"))?;
            collect_record(payload, contents);
        }
    }
    Ok(())
}

fn collect_record(record: RecordPayload, contents: &mut BTreeSet<String>) {
    contents.insert(record.item.content);
    if let Some(source) = record.source {
        collect_source_text(&source, contents);
    }
}

fn collect_source_text(value: &serde_json::Value, contents: &mut BTreeSet<String>) {
    match value {
        serde_json::Value::Object(fields) => {
            for (key, value) in fields {
                collect_source_field(key, value, contents);
                collect_source_text(value, contents);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_source_text(item, contents);
            }
        }
        _ => {}
    }
}

fn collect_source_field(key: &str, value: &serde_json::Value, contents: &mut BTreeSet<String>) {
    if matches!(key, "content" | "resolvedContent" | "sourceExcerpt") {
        if let Some(text) = value.as_str() {
            contents.insert(text.to_owned());
        }
    }
    if key == "wordingVariants" {
        contents.extend(
            value
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|value| value.as_str().map(str::to_owned)),
        );
    }
}
