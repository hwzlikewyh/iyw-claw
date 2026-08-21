use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// A JetBrains AIR typed session-failure upsert.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionFailureRecord {
    pub id: String,
    pub revision: u64,
    pub category: String,
    pub severity: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<String>,
    #[serde(default)]
    pub resolved: bool,
}

/// Monotonic record table retained as the revision watermark for each id.
#[derive(Debug, Clone, Default)]
pub struct SessionFailureTable {
    records: BTreeMap<String, SessionFailureRecord>,
}

impl SessionFailureTable {
    pub fn upsert(&mut self, mut record: SessionFailureRecord) -> bool {
        let accepted = self
            .records
            .get(&record.id)
            .is_none_or(|stored| record.revision > stored.revision);
        if !accepted {
            return false;
        }
        record.resolved = false;
        self.records.insert(record.id.clone(), record);
        true
    }

    pub fn settle_warnings(&mut self) -> bool {
        let mut changed = false;
        for record in self.records.values_mut() {
            if !record.resolved && record.severity == "warning" {
                record.resolved = true;
                changed = true;
            }
        }
        changed
    }

    pub fn settle_retry_incidents(&mut self) -> bool {
        let mut changed = false;
        for record in self.records.values_mut() {
            if !record.resolved && record.severity == "warning" && record.category != "unknown" {
                record.resolved = true;
                changed = true;
            }
        }
        changed
    }

    pub fn settle_all(&mut self) -> bool {
        let mut changed = false;
        for record in self.records.values_mut() {
            if !record.resolved {
                record.resolved = true;
                changed = true;
            }
        }
        changed
    }

    pub fn snapshot(&self) -> Vec<SessionFailureRecord> {
        self.records.values().cloned().collect()
    }
}

/// Read a versioned AIR failure envelope from an ACP `_meta` object.
pub fn from_air_meta(meta: Option<&Map<String, Value>>) -> Option<SessionFailureRecord> {
    let air = meta?.get("jetbrains")?.get("air")?;
    let version = air.get("version")?.as_i64()?;
    if version < 1 {
        return None;
    }
    let record = parse_record(air.get("sessionFailure")?)?;
    if record.severity.eq_ignore_ascii_case("warning") {
        tracing::debug!(
            failure_id = %record.id,
            category = %record.category,
            "[ACP] suppressing non-terminal AIR warning from conversation banner"
        );
        return None;
    }
    Some(record)
}

/// Validate stable identity while accepting future category/action vocabulary.
pub fn parse_record(value: &Value) -> Option<SessionFailureRecord> {
    let id = value.get("id")?.as_str()?.trim();
    if id.is_empty() {
        return None;
    }
    let revision = value.get("revision")?.as_u64()?;
    if revision < 1 {
        return None;
    }

    Some(SessionFailureRecord {
        id: id.to_string(),
        revision,
        category: text(value, "category", "unknown"),
        severity: text(value, "severity", "error"),
        title: text(value, "title", ""),
        details: optional_text(value, "details"),
        actions: value
            .get("actions")
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        resolved: false,
    })
}

fn text(value: &Value, key: &str, default: &str) -> String {
    optional_text(value, key).unwrap_or_else(|| default.to_string())
}

fn optional_text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
}
