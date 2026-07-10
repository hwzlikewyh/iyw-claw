use chrono::{Duration, NaiveDate};
use serde::{Deserialize, Serialize};

use super::TurnUsage;

const DASHBOARD_DAY_COUNT: i64 = 30;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageBreakdown {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

impl UsageBreakdown {
    fn from_turn_usage(usage: &TurnUsage) -> Self {
        Self {
            input: usage.input_tokens,
            output: usage.output_tokens,
            cache_read: usage.cache_read_input_tokens,
            cache_write: usage.cache_creation_input_tokens,
        }
    }

    pub fn total(&self) -> u64 {
        self.input + self.output + self.cache_read + self.cache_write
    }

    fn apply(&mut self, other: &Self, add: bool) {
        if add {
            self.input = self.input.saturating_add(other.input);
            self.output = self.output.saturating_add(other.output);
            self.cache_read = self.cache_read.saturating_add(other.cache_read);
            self.cache_write = self.cache_write.saturating_add(other.cache_write);
        } else {
            self.input = self.input.saturating_sub(other.input);
            self.output = self.output.saturating_sub(other.output);
            self.cache_read = self.cache_read.saturating_sub(other.cache_read);
            self.cache_write = self.cache_write.saturating_sub(other.cache_write);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageModelRow {
    pub model: String,
    pub sessions: u64,
    pub total: u64,
    #[serde(flatten)]
    pub usage: UsageBreakdown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageDailyRow {
    pub date: String,
    pub sessions: u64,
    pub total: u64,
    pub cache_hit_rate: f64,
    #[serde(flatten)]
    pub usage: UsageBreakdown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageDashboardStats {
    pub total: UsageBreakdown,
    pub total_tokens: u64,
    pub session_count: u64,
    pub cache_hit_rate: f64,
    pub average_daily_sessions: f64,
    pub first_date: Option<String>,
    pub last_date: Option<String>,
    pub model_rows: Vec<UsageModelRow>,
    pub daily_rows: Vec<UsageDailyRow>,
}

impl Default for UsageDashboardStats {
    fn default() -> Self {
        Self {
            total: UsageBreakdown::default(),
            total_tokens: 0,
            session_count: 0,
            cache_hit_rate: 0.0,
            average_daily_sessions: 0.0,
            first_date: None,
            last_date: None,
            model_rows: Vec::new(),
            daily_rows: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionUsageSnapshot {
    pub conversation_id: i32,
    pub date: String,
    pub model: String,
    pub usage: TurnUsage,
}

impl UsageDashboardStats {
    pub(crate) fn replace_session(
        &mut self,
        previous: Option<&SessionUsageSnapshot>,
        next: &SessionUsageSnapshot,
    ) {
        if let Some(previous) = previous {
            self.apply_session(previous, false);
        }
        self.apply_session(next, true);
        self.remember_date(&next.date);
        self.refresh_derived();
    }

    fn apply_session(&mut self, snapshot: &SessionUsageSnapshot, add: bool) {
        let usage = UsageBreakdown::from_turn_usage(&snapshot.usage);
        self.total.apply(&usage, add);
        self.session_count = if add {
            self.session_count.saturating_add(1)
        } else {
            self.session_count.saturating_sub(1)
        };
        apply_model_row(&mut self.model_rows, snapshot, &usage, add);
        apply_daily_row(&mut self.daily_rows, snapshot, &usage, add);
    }

    fn remember_date(&mut self, date: &str) {
        if self.first_date.as_deref().is_none_or(|first| date < first) {
            self.first_date = Some(date.to_string());
        }
        if self.last_date.as_deref().is_none_or(|last| date > last) {
            self.last_date = Some(date.to_string());
        }
    }

    fn refresh_derived(&mut self) {
        self.total_tokens = self.total.total();
        self.cache_hit_rate = cache_hit_rate(&self.total);
        self.model_rows
            .sort_by(|a, b| b.total.cmp(&a.total).then_with(|| a.model.cmp(&b.model)));
        self.daily_rows = normalized_daily_rows(
            std::mem::take(&mut self.daily_rows),
            self.first_date.as_deref(),
            self.last_date.as_deref(),
        );
        self.average_daily_sessions = average_daily_sessions(
            self.session_count,
            self.first_date.as_deref(),
            self.last_date.as_deref(),
        );
    }
}

fn apply_model_row(
    rows: &mut Vec<UsageModelRow>,
    snapshot: &SessionUsageSnapshot,
    usage: &UsageBreakdown,
    add: bool,
) {
    let index = rows.iter().position(|row| row.model == snapshot.model);
    if index.is_none() && add {
        rows.push(UsageModelRow {
            model: snapshot.model.clone(),
            sessions: 0,
            total: 0,
            usage: UsageBreakdown::default(),
        });
    }
    let Some(index) = index.or_else(|| rows.len().checked_sub(1).filter(|_| add)) else {
        return;
    };
    let row = &mut rows[index];
    apply_row(&mut row.sessions, &mut row.usage, usage, add);
    row.total = row.usage.total();
    if row.sessions == 0 {
        rows.remove(index);
    }
}

fn apply_daily_row(
    rows: &mut Vec<UsageDailyRow>,
    snapshot: &SessionUsageSnapshot,
    usage: &UsageBreakdown,
    add: bool,
) {
    let index = rows.iter().position(|row| row.date == snapshot.date);
    if index.is_none() && add {
        rows.push(empty_daily_row(snapshot.date.clone()));
    }
    let Some(index) = index.or_else(|| rows.len().checked_sub(1).filter(|_| add)) else {
        return;
    };
    let row = &mut rows[index];
    apply_row(&mut row.sessions, &mut row.usage, usage, add);
    row.total = row.usage.total();
    row.cache_hit_rate = cache_hit_rate(&row.usage);
    if row.sessions == 0 {
        rows.remove(index);
    }
}

fn apply_row(sessions: &mut u64, target: &mut UsageBreakdown, usage: &UsageBreakdown, add: bool) {
    *sessions = if add {
        sessions.saturating_add(1)
    } else {
        sessions.saturating_sub(1)
    };
    target.apply(usage, add);
}

fn cache_hit_rate(usage: &UsageBreakdown) -> f64 {
    let prompt_tokens = usage.input + usage.cache_read + usage.cache_write;
    if prompt_tokens == 0 {
        0.0
    } else {
        usage.cache_read as f64 / prompt_tokens as f64
    }
}

fn empty_daily_row(date: String) -> UsageDailyRow {
    UsageDailyRow {
        date,
        sessions: 0,
        total: 0,
        cache_hit_rate: 0.0,
        usage: UsageBreakdown::default(),
    }
}

fn normalized_daily_rows(
    rows: Vec<UsageDailyRow>,
    first_date: Option<&str>,
    last_date: Option<&str>,
) -> Vec<UsageDailyRow> {
    let (Some(first), Some(last)) = (parse_date(first_date), parse_date(last_date)) else {
        return rows;
    };
    let start = std::cmp::max(first, last - Duration::days(DASHBOARD_DAY_COUNT - 1));
    let by_date = rows
        .into_iter()
        .map(|row| (row.date.clone(), row))
        .collect::<std::collections::HashMap<_, _>>();
    (0..=(last - start).num_days())
        .map(|offset| {
            let date = (start + Duration::days(offset))
                .format("%Y-%m-%d")
                .to_string();
            by_date
                .get(&date)
                .cloned()
                .unwrap_or_else(|| empty_daily_row(date))
        })
        .collect()
}

fn average_daily_sessions(count: u64, first: Option<&str>, last: Option<&str>) -> f64 {
    let (Some(first), Some(last)) = (parse_date(first), parse_date(last)) else {
        return 0.0;
    };
    let days = (last - first).num_days().max(0) + 1;
    count as f64 / days as f64
}

fn parse_date(value: Option<&str>) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value?, "%Y-%m-%d").ok()
}
