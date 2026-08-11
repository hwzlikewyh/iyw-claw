use std::sync::OnceLock;
use std::time::Duration;

use sea_orm::DatabaseConnection;
use serde::Deserialize;

use crate::app_error::AppCommandError;
#[cfg(feature = "tauri-runtime")]
use crate::db::AppDatabase;
use crate::models::{AgentType, UsageBreakdown, UsageDailyRow, UsageDashboardStats, UsageModelRow};

const USAGE_CURRENCY: &str = "CNY";
const USAGE_LIMIT: usize = 30;
const USAGE_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Deserialize)]
struct FusionResponse<T> {
    code: i32,
    message: Option<String>,
    data: Option<T>,
}

#[derive(Debug, Deserialize)]
struct FusionUsageData {
    #[serde(default)]
    items: Vec<FusionDailyUsage>,
    #[serde(default)]
    model_items: Vec<FusionModelUsage>,
    summary: FusionUsageSummary,
}

#[derive(Debug, Deserialize)]
struct FusionUsageSummary {
    sessions: u64,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
    total_tokens: u64,
    cache_hit_rate: f64,
    average_daily_sessions: f64,
    first_date: String,
    last_date: String,
    total_cost: f64,
    currency: String,
}

#[derive(Debug, Deserialize)]
struct FusionDailyUsage {
    date: String,
    sessions: u64,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
    total_tokens: u64,
    cache_hit_rate: f64,
    total_cost: f64,
    currency: String,
}

#[derive(Debug, Deserialize)]
struct FusionModelUsage {
    model: String,
    sessions: u64,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
    total_tokens: u64,
    total_cost: f64,
    currency: String,
}

impl From<FusionDailyUsage> for UsageDailyRow {
    fn from(value: FusionDailyUsage) -> Self {
        Self {
            date: value.date,
            sessions: value.sessions,
            total: value.total_tokens,
            cache_hit_rate: value.cache_hit_rate,
            total_cost: value.total_cost,
            currency: value.currency,
            usage: usage_breakdown(
                value.input_tokens,
                value.output_tokens,
                value.cache_read_tokens,
                value.cache_write_tokens,
            ),
        }
    }
}

impl From<FusionModelUsage> for UsageModelRow {
    fn from(value: FusionModelUsage) -> Self {
        Self {
            model: value.model,
            sessions: value.sessions,
            total: value.total_tokens,
            total_cost: value.total_cost,
            currency: value.currency,
            usage: usage_breakdown(
                value.input_tokens,
                value.output_tokens,
                value.cache_read_tokens,
                value.cache_write_tokens,
            ),
        }
    }
}

impl FusionUsageData {
    fn into_dashboard(self) -> UsageDashboardStats {
        let mut daily_rows = self
            .items
            .into_iter()
            .map(UsageDailyRow::from)
            .collect::<Vec<_>>();
        daily_rows.sort_by(|left, right| left.date.cmp(&right.date));
        let summary = self.summary;
        UsageDashboardStats {
            total: usage_breakdown(
                summary.input_tokens,
                summary.output_tokens,
                summary.cache_read_tokens,
                summary.cache_write_tokens,
            ),
            total_tokens: summary.total_tokens,
            session_count: summary.sessions,
            cache_hit_rate: summary.cache_hit_rate,
            average_daily_sessions: summary.average_daily_sessions,
            total_cost: summary.total_cost,
            currency: summary.currency,
            first_date: non_empty(summary.first_date),
            last_date: non_empty(summary.last_date),
            model_rows: self
                .model_items
                .into_iter()
                .map(UsageModelRow::from)
                .collect(),
            daily_rows,
        }
    }
}

fn usage_breakdown(input: u64, output: u64, cache_read: u64, cache_write: u64) -> UsageBreakdown {
    UsageBreakdown {
        input,
        output,
        cache_read,
        cache_write,
    }
}

fn non_empty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}

fn usage_url() -> String {
    let base = crate::acp::provider_overlay::model_gateway_base_url_for(AgentType::Codex);
    let base = base.trim_end_matches('/');
    if base.ends_with("/v1") {
        format!("{base}/usage/recent?limit={USAGE_LIMIT}&currency={USAGE_CURRENCY}")
    } else {
        format!("{base}/v1/usage/recent?limit={USAGE_LIMIT}&currency={USAGE_CURRENCY}")
    }
}

fn http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}

pub async fn get_usage_dashboard_core(
    conn: &DatabaseConnection,
) -> Result<UsageDashboardStats, AppCommandError> {
    let token = crate::commands::iyw_account::iyw_account_access_token_core(conn)
        .await?
        .ok_or_else(|| AppCommandError::authentication_failed("Sign in to view usage"))?;
    tracing::debug!(
        limit = USAGE_LIMIT,
        currency = USAGE_CURRENCY,
        "requesting actual usage dashboard"
    );
    let response = http_client()
        .get(usage_url())
        .timeout(USAGE_TIMEOUT)
        .header("token", token.expose())
        .send()
        .await
        .map_err(|error| {
            AppCommandError::network("Failed to load usage").with_detail(error.to_string())
        })?;
    let status = response.status();
    if !status.is_success() {
        tracing::warn!(status = status.as_u16(), "usage request failed");
        return Err(AppCommandError::network("Usage request failed")
            .with_detail(format!("HTTP {}", status.as_u16())));
    }
    let payload = response
        .json::<FusionResponse<FusionUsageData>>()
        .await
        .map_err(|error| {
            AppCommandError::network("Usage response was invalid").with_detail(error.to_string())
        })?;
    if payload.code != 1 {
        tracing::warn!(business_code = payload.code, "usage request was rejected");
        return Err(
            AppCommandError::network("Usage request was rejected").with_detail(
                payload
                    .message
                    .unwrap_or_else(|| "Unknown error".to_string()),
            ),
        );
    }
    let data = payload
        .data
        .ok_or_else(|| AppCommandError::network("Usage response did not contain data"))?;
    tracing::info!(
        daily_rows = data.items.len(),
        model_rows = data.model_items.len(),
        requests = data.summary.sessions,
        "actual usage dashboard loaded"
    );
    Ok(data.into_dashboard())
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn get_usage_dashboard(
    db: tauri::State<'_, AppDatabase>,
) -> Result<UsageDashboardStats, AppCommandError> {
    get_usage_dashboard_core(&db.conn).await
}
