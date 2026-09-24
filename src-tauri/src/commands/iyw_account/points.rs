use super::{AppCommandError, FUSION_API_BASE_URL};
use serde::Deserialize;

#[derive(Deserialize)]
struct PointsEnvelope {
    code: i32,
    data: PointsData,
}

#[derive(Deserialize)]
struct PointsData {
    available_points: Option<f64>,
    #[serde(rename = "errorCode")]
    error_code: Option<String>,
}

pub(super) async fn fetch(client: &reqwest::Client, token: &str) -> Result<f64, AppCommandError> {
    let response = client
        .get(format!("{FUSION_API_BASE_URL}/v1/account/points"))
        .header("Accept", "application/json")
        .header("token", token)
        .send()
        .await
        .map_err(|error| {
            unavailable().with_detail(format!("stage=request error={}", error.without_url()))
        })?;
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(unavailable().with_detail(format!(
            "stage=authentication http_status={}",
            status.as_u16()
        )));
    }
    if !status.is_success() {
        return Err(unavailable().with_detail(format!("http_status={}", status.as_u16())));
    }
    let result = response.json::<PointsEnvelope>().await.map_err(|_| {
        unavailable().with_detail(format!(
            "stage=response_decode http_status={}",
            status.as_u16()
        ))
    })?;
    if result.data.error_code.as_deref() == Some("authentication_failed") {
        return Err(unavailable().with_detail(format!(
            "stage=authentication http_status={} business_code={} error_code=authentication_failed",
            status.as_u16(),
            result.code
        )));
    }
    if result.code != 1 {
        return Err(unavailable().with_detail(format!(
            "stage=response http_status={} business_code={}",
            status.as_u16(),
            result.code
        )));
    }
    result
        .data
        .available_points
        .filter(|value| value.is_finite() && *value >= 0.0)
        .ok_or_else(|| unavailable().with_detail("stage=balance_decode missing or invalid balance"))
}

fn unavailable() -> AppCommandError {
    AppCommandError::network("Failed to load iyw available points")
}
