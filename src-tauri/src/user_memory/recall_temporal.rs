use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate};
use sea_orm::{ConnectionTrait, DbBackend, Statement};

use crate::app_error::AppCommandError;

use super::recall_query::{LaneCollection, RecallQuery};
use super::recall_rank::{add_rows, Candidate, LaneScore};
use super::recall_status::database_error;
use super::recall_validity::{push_query_at, valid_at_sql};

pub(super) const MAX_TEMPORAL_CANDIDATES: usize = 24;

pub(super) async fn collect_temporal<C: ConnectionTrait>(
    db: &C,
    query: RecallQuery<'_>,
    out: &mut BTreeMap<String, Candidate>,
) -> Result<LaneCollection, AppCommandError> {
    let Some(range) = temporal_range(query.query()) else {
        return Ok(LaneCollection::skipped("query_has_no_date"));
    };
    let validity = valid_at_sql("i");
    let scope = query.scope().predicate("i");
    let sql = format!(
        "SELECT e.memory_id FROM memory_evidence AS e INDEXED BY idx_memory_evidence_time CROSS JOIN memory_item_current AS i ON i.id = e.memory_id WHERE e.observed_at >= ? AND e.observed_at < ? AND {scope} AND i.trust_class = 'host_confirmed' AND i.sensitive = 0 AND i.superseded_by IS NULL{validity} GROUP BY e.memory_id ORDER BY MAX(e.observed_at) DESC, e.memory_id LIMIT ?"
    );
    let mut values = vec![range.start.into(), range.end.into()];
    query.scope().push_bind(&mut values);
    push_query_at(&mut values, query.query_at());
    values.push((MAX_TEMPORAL_CANDIDATES as i64).into());
    let rows = db
        .query_all(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            sql,
            values,
        ))
        .await
        .map_err(database_error)?;
    let candidate_count = rows.len();
    add_rows(
        rows,
        out,
        LaneScore {
            name: "temporal",
            weight: 0.5,
        },
    );
    Ok(LaneCollection::collected(candidate_count))
}

struct TemporalRange {
    start: String,
    end: String,
}

fn temporal_range(value: &str) -> Option<TemporalRange> {
    let key = temporal_key(value)?;
    let start = if key.len() == 10 {
        NaiveDate::parse_from_str(&key, "%Y-%m-%d").ok()?
    } else {
        NaiveDate::parse_from_str(&format!("{key}-01"), "%Y-%m-%d").ok()?
    };
    let end = if key.len() == 10 {
        start.succ_opt()?
    } else {
        next_month(start)?
    };
    Some(TemporalRange {
        start: utc_midnight(start),
        end: utc_midnight(end),
    })
}

fn next_month(value: NaiveDate) -> Option<NaiveDate> {
    if value.month() == 12 {
        NaiveDate::from_ymd_opt(value.year().checked_add(1)?, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(value.year(), value.month() + 1, 1)
    }
}

fn utc_midnight(value: NaiveDate) -> String {
    format!("{value}T00:00:00Z")
}

pub(super) fn temporal_key(value: &str) -> Option<String> {
    find_iso_date(value).or_else(|| find_iso_month(value))
}

pub(super) fn is_pure_temporal_query(value: &str) -> bool {
    let value = value.trim();
    temporal_key(value).as_deref() == Some(value)
}

fn find_iso_date(value: &str) -> Option<String> {
    find_ascii_pattern(value, 10, |candidate| {
        candidate.as_bytes().get(4) == Some(&b'-')
            && candidate.as_bytes().get(7) == Some(&b'-')
            && NaiveDate::parse_from_str(candidate, "%Y-%m-%d").is_ok()
    })
}

fn find_iso_month(value: &str) -> Option<String> {
    find_ascii_pattern(value, 7, |candidate| {
        candidate.as_bytes().get(4) == Some(&b'-')
            && NaiveDate::parse_from_str(&format!("{candidate}-01"), "%Y-%m-%d").is_ok()
    })
}

fn find_ascii_pattern(value: &str, width: usize, valid: impl Fn(&str) -> bool) -> Option<String> {
    let bytes = value.as_bytes();
    bytes
        .windows(width)
        .enumerate()
        .find_map(|(start, window)| {
            let end = start + width;
            if has_identifier_neighbor(bytes, start, end)
                || has_date_continuation(bytes, end)
                || (width == 7 && bytes.get(end) == Some(&b'-'))
                || !window
                    .iter()
                    .enumerate()
                    .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
            {
                return None;
            }
            let candidate = std::str::from_utf8(window).ok()?;
            valid(candidate).then(|| candidate.to_string())
        })
}

fn has_identifier_neighbor(bytes: &[u8], start: usize, end: usize) -> bool {
    let is_identifier = |byte: &u8| byte.is_ascii_alphanumeric() || matches!(*byte, b'_' | b'#');
    start
        .checked_sub(1)
        .and_then(|index| bytes.get(index))
        .is_some_and(is_identifier)
        || bytes.get(end).is_some_and(is_identifier)
}

fn has_date_continuation(bytes: &[u8], end: usize) -> bool {
    bytes.get(end) == Some(&b'-') && bytes.get(end + 1).is_some_and(u8::is_ascii_digit)
}

#[cfg(test)]
mod tests {
    use super::{temporal_key, temporal_range};

    #[test]
    fn date_range_uses_utc_half_open_day() {
        let range = temporal_range("what happened on 2024-02-29").unwrap();
        assert_eq!(range.start, "2024-02-29T00:00:00Z");
        assert_eq!(range.end, "2024-03-01T00:00:00Z");
    }

    #[test]
    fn month_range_rolls_over_the_year() {
        let range = temporal_range("notes from 2025-12").unwrap();
        assert_eq!(range.start, "2025-12-01T00:00:00Z");
        assert_eq!(range.end, "2026-01-01T00:00:00Z");
    }

    #[test]
    fn source_fallback_key_semantics_remain_unchanged() {
        assert_eq!(temporal_key("v2025-07 release"), None);
        assert_eq!(temporal_key("during 2025-07"), Some("2025-07".to_string()));
        assert!(temporal_range("during 2025-13").is_none());
    }
}
