use super::{
    authority_records::RecordPayload,
    helpers,
    restore_bundle::{invalid, RecoveryBundle},
};
use crate::app_error::AppCommandError;
use std::collections::BTreeMap;

pub(super) fn validate_history(bundle: &RecoveryBundle) -> Result<(), AppCommandError> {
    let mut versions = BTreeMap::new();
    for row in &bundle.tables[1] {
        let key = (text(row, 0)?, number(row, 1)?);
        let payload: RecordPayload = serde_json::from_str(text(row, 5)?)
            .map_err(|_| invalid("Invalid historical payload"))?;
        if payload.item.id != text(row, 2)?
            || payload.state != text(row, 4)?
            || helpers::hash_parts(&[payload.item.content.as_bytes()]) != text(row, 3)?
            || versions.insert(key, row).is_some()
        {
            return Err(invalid("Recovery revision integrity mismatch"));
        }
    }
    let mut records = BTreeMap::new();
    for row in &bundle.tables[0] {
        let id = text(row, 0)?;
        let revision = number(row, 2)?;
        let value = versions
            .get(&(id, revision))
            .ok_or_else(|| invalid("Current memory revision is missing"))?;
        if text(value, 2)? != text(row, 1)?
            || text(value, 4)? != text(row, 5)?
            || helpers::hash_parts(&[text(value, 5)?.as_bytes()]) != text(row, 4)?
            || records.insert(id, revision).is_some()
        {
            return Err(invalid("Recovery record integrity mismatch"));
        }
        if (1..=revision).any(|version| !versions.contains_key(&(id, version))) {
            return Err(invalid("Memory history has missing versions"));
        }
    }
    if versions
        .keys()
        .any(|(id, version)| records.get(id).is_none_or(|latest| version > latest))
    {
        return Err(invalid("Recovery has orphan revisions"));
    }
    validate_identities(bundle, &records)
}

fn validate_identities(
    bundle: &RecoveryBundle,
    records: &BTreeMap<&str, i64>,
) -> Result<(), AppCommandError> {
    let mut identities = BTreeMap::new();
    for row in &bundle.tables[2] {
        let id = text(row, 0)?;
        let stable = text(row, 1)?;
        if !records.contains_key(stable) || identities.insert(id, stable).is_some() {
            return Err(invalid("Recovery identity mismatch"));
        }
    }
    for row in &bundle.tables[0] {
        if identities.get(text(row, 1)?) != Some(&text(row, 0)?) {
            return Err(invalid("Recovery current identity missing"));
        }
    }
    Ok(())
}

pub(super) fn text(row: &[serde_json::Value], index: usize) -> Result<&str, AppCommandError> {
    row.get(index)
        .and_then(|value| value.as_str())
        .ok_or_else(|| invalid("Recovery text field missing"))
}

pub(super) fn number(row: &[serde_json::Value], index: usize) -> Result<i64, AppCommandError> {
    row.get(index)
        .and_then(|value| value.as_i64())
        .filter(|value| *value > 0 && *value <= 65_536)
        .ok_or_else(|| invalid("Recovery revision field invalid"))
}

pub(super) fn preserves(
    current: &RecoveryBundle,
    source: &RecoveryBundle,
) -> Result<(), AppCommandError> {
    if current.snapshot.store_id != source.snapshot.store_id
        || current.snapshot.epoch > source.snapshot.epoch
        || current.snapshot.epoch == source.snapshot.epoch
            && current.snapshot.digest != source.snapshot.digest
    {
        return Err(invalid(
            "Recovery cannot replace a newer or different memory store",
        ));
    }
    for index in [1, 2] {
        for row in &current.tables[index] {
            if !source.tables[index].contains(row) {
                return Err(invalid("Recovery would lose or rewrite existing history"));
            }
        }
    }
    Ok(())
}

pub(super) fn matches_snapshot(
    service: &super::UserMemoryService,
    bundle: &RecoveryBundle,
) -> Result<(), AppCommandError> {
    let payloads = super::authority_inventory::records(service, &bundle.snapshot.data)?;
    let mut expected = BTreeMap::new();
    for payload in payloads {
        let digest = helpers::hash_parts(&[super::authority::encode(&payload)?.as_bytes()]);
        expected.insert(payload.item.id, digest);
    }
    for row in &bundle.tables[0] {
        if text(row, 5)? == "retracted" {
            continue;
        }
        if expected.remove(text(row, 1)?).as_deref() != Some(text(row, 4)?) {
            return Err(invalid(
                "Recovery records differ from the authoritative snapshot",
            ));
        }
    }
    if !expected.is_empty() {
        return Err(invalid("Recovery is missing authoritative records"));
    }
    Ok(())
}
