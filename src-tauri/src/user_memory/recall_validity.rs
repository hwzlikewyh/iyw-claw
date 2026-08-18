use chrono::{DateTime, Utc};
use sea_orm::Value;

use super::index_types::IndexItem;

pub(super) fn valid_at_sql(alias: &str) -> String {
    // Indexed timestamps are canonical UTC RFC3339 values. Lexicographic
    // comparison preserves chronological order and keeps the scope/time
    // indexes usable; julianday() forced a per-row function scan.
    format!(
        " AND ({alias}.valid_from IS NULL OR {alias}.valid_from <= ?) AND ({alias}.valid_to IS NULL OR {alias}.valid_to > ?)"
    )
}

pub(super) fn push_query_at(values: &mut Vec<Value>, query_at: &str) {
    values.push(query_at.to_string().into());
    values.push(query_at.to_string().into());
}

pub(super) fn item_is_current_at(item: &IndexItem, query_at: &DateTime<Utc>) -> bool {
    lower_bound_allows(item.valid_from.as_deref(), query_at)
        && upper_bound_allows(item.valid_to.as_deref(), query_at)
}

fn lower_bound_allows(value: Option<&str>, query_at: &DateTime<Utc>) -> bool {
    value
        .map(|value| parse_utc(value).is_some_and(|bound| bound <= *query_at))
        .unwrap_or(true)
}

fn upper_bound_allows(value: Option<&str>, query_at: &DateTime<Utc>) -> bool {
    value
        .map(|value| parse_utc(value).is_some_and(|bound| bound > *query_at))
        .unwrap_or(true)
}

fn parse_utc(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .ok()
}
