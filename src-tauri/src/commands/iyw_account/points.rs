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
        .map_err(|_| unavailable())?;
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(AppCommandError::authentication_failed(
            "IYW account login expired",
        ));
    }
    if !response.status().is_success() {
        return Err(unavailable().with_detail(response.status().to_string()));
    }
    let result = response
        .json::<PointsEnvelope>()
        .await
        .map_err(|_| unavailable())?;
    if result.data.error_code.as_deref() == Some("authentication_failed") {
        return Err(AppCommandError::authentication_failed(
            "IYW account login expired",
        ));
    }
    if result.code != 1 {
        return Err(unavailable());
    }
    result
        .data
        .available_points
        .filter(|value| value.is_finite() && *value >= 0.0)
        .ok_or_else(unavailable)
}

fn unavailable() -> AppCommandError {
    AppCommandError::network("Failed to load iyw available points")
}
