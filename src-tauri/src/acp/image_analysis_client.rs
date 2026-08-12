use std::sync::OnceLock;
use std::time::Duration;

use serde_json::{json, Value};

use super::image_analysis::AnalysisRequest;

const ANALYSIS_TIMEOUT: Duration = Duration::from_secs(90);

pub async fn call_fusion(
    token: &crate::acp::account_credentials::AccountAccessToken,
    model: &str,
    request: &AnalysisRequest,
) -> Value {
    let images = request
        .images
        .iter()
        .map(|image| match image.url.as_deref() {
            Some(url) => json!({ "url": url }),
            None => json!({ "data": image.data, "mime_type": image.mime_type }),
        })
        .collect::<Vec<_>>();
    let response = match client()
        .post(crate::acp::provider_overlay::model_gateway_image_analysis_url())
        .timeout(ANALYSIS_TIMEOUT)
        .header("token", token.expose())
        .json(&json!({
            "model": model, "images": images,
            "question": request.question, "detail": request.detail,
        }))
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!(error = %error, "[ImageAnalysis] Fusion request failed");
            return error_value(
                "image_analysis_transport_failed",
                "The image analysis service could not be reached.",
            );
        }
    };
    let status = response.status();
    let payload = match response.json::<Value>().await {
        Ok(payload) => payload,
        Err(_) => {
            return error_value(
                "image_analysis_invalid_response",
                "The image analysis service returned an invalid response.",
            )
        }
    };
    if status.is_success() {
        payload
    } else {
        fusion_error(payload, status.as_u16())
    }
}

fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}

fn fusion_error(payload: Value, status: u16) -> Value {
    let code = payload
        .pointer("/error/code")
        .and_then(Value::as_str)
        .unwrap_or("image_analysis_failed");
    let message = payload
        .pointer("/error/message")
        .and_then(Value::as_str)
        .unwrap_or("Image analysis failed.");
    json!({ "error": message, "code": code, "status": status })
}

fn error_value(code: &str, message: &str) -> Value {
    json!({ "error": message, "code": code })
}
