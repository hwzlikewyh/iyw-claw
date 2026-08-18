use std::collections::BTreeSet;

use sea_orm::{ConnectionTrait, DbBackend, Statement};

use super::index_types::{normalize_alias, IndexItem, IndexRelation, IndexSnapshot};

pub(super) fn validate_snapshot_identities(snapshot: &IndexSnapshot) -> Result<(), sea_orm::DbErr> {
    let mut item_ids = BTreeSet::new();
    for item in &snapshot.items {
        if item.id.is_empty() || !item_ids.insert(item.id.as_str()) {
            return Err(identity_error("item"));
        }
        validate_alias_identities(item)?;
        validate_evidence_identities(item)?;
    }
    validate_relations(snapshot, &item_ids)?;
    Ok(())
}

pub(super) async fn validate_current_rows<C: ConnectionTrait>(
    conn: &C,
    snapshot: &IndexSnapshot,
) -> Result<(), sea_orm::DbErr> {
    let expected = [
        ("memory_item_current", snapshot.items.len()),
        (
            "memory_alias_current",
            snapshot.items.iter().map(|item| item.aliases.len()).sum(),
        ),
        (
            "memory_evidence",
            snapshot.items.iter().map(|item| item.evidence.len()).sum(),
        ),
        ("memory_relation_current", snapshot.relations.len()),
    ];
    for (table, expected_count) in expected {
        let actual = table_count(conn, table).await?;
        if actual != expected_count as i64 {
            return Err(sea_orm::DbErr::Custom(format!(
                "memory index row count mismatch for {table}: expected {expected_count}, got {actual}"
            )));
        }
    }
    Ok(())
}

pub(super) async fn check_fts_integrity<C: ConnectionTrait>(
    conn: &C,
    table: &str,
) -> Result<(), sea_orm::DbErr> {
    conn.execute(Statement::from_string(
        DbBackend::Sqlite,
        format!("INSERT INTO {table}({table}, rank) VALUES ('integrity-check', 1)"),
    ))
    .await
    .map(|_| ())
}

async fn table_count<C: ConnectionTrait>(conn: &C, table: &str) -> Result<i64, sea_orm::DbErr> {
    let row = conn
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            format!("SELECT COUNT(*) AS row_count FROM {table}"),
        ))
        .await?
        .ok_or_else(|| sea_orm::DbErr::Custom(format!("missing row count for {table}")))?;
    row.try_get("", "row_count")
}

fn validate_alias_identities(item: &IndexItem) -> Result<(), sea_orm::DbErr> {
    let mut identities = BTreeSet::new();
    for alias in &item.aliases {
        let normalized = normalize_alias(&alias.value);
        if alias.kind.trim().is_empty() || normalized.is_empty() || !identities.insert(normalized) {
            return Err(identity_error("alias"));
        }
    }
    Ok(())
}

fn validate_evidence_identities(item: &IndexItem) -> Result<(), sea_orm::DbErr> {
    let mut identities = BTreeSet::new();
    for evidence in &item.evidence {
        let identity = (
            evidence.source_kind.as_str(),
            evidence.source_id.as_str(),
            evidence.turn_nonce,
        );
        if evidence.source_kind.trim().is_empty()
            || evidence.source_id.trim().is_empty()
            || !identities.insert(identity)
        {
            return Err(identity_error("evidence"));
        }
    }
    Ok(())
}

fn validate_relations(
    snapshot: &IndexSnapshot,
    item_ids: &BTreeSet<&str>,
) -> Result<(), sea_orm::DbErr> {
    let mut identities = BTreeSet::new();
    for relation in &snapshot.relations {
        validate_relation(relation, item_ids)?;
        let identity = (
            relation.source_id.as_str(),
            relation.relation.as_str(),
            relation.target_id.as_str(),
        );
        if !identities.insert(identity) {
            return Err(identity_error("relation"));
        }
    }
    for relation in snapshot
        .relations
        .iter()
        .filter(|relation| relation.relation == "contradicts")
    {
        let reverse = (
            relation.target_id.as_str(),
            relation.relation.as_str(),
            relation.source_id.as_str(),
        );
        if !identities.contains(&reverse) {
            return Err(identity_error("contradiction_pair"));
        }
    }
    Ok(())
}

fn validate_relation(
    relation: &IndexRelation,
    item_ids: &BTreeSet<&str>,
) -> Result<(), sea_orm::DbErr> {
    let known = matches!(
        relation.relation.as_str(),
        "supports" | "relates_to" | "related" | "contradicts"
    );
    if !known
        || relation.source_id == relation.target_id
        || !item_ids.contains(relation.source_id.as_str())
        || !item_ids.contains(relation.target_id.as_str())
        || !(0..=100).contains(&relation.confidence)
        || relation.created_at.trim().is_empty()
    {
        return Err(identity_error("relation"));
    }
    Ok(())
}

fn identity_error(kind: &str) -> sea_orm::DbErr {
    sea_orm::DbErr::Custom(format!(
        "memory index snapshot contains duplicate or empty {kind} identity"
    ))
}
