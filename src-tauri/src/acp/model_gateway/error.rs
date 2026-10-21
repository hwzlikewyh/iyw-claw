use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

pub(super) struct GatewayError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: &'static str,
}

impl GatewayError {
    pub fn cloud(error: reqwest::Error) -> Self {
        // 只保留错误类别；reqwest 原文可能包含带签名的地址。
        use std::error::Error;
        let cause = error.source().map(ToString::to_string).unwrap_or_default().to_lowercase();
        let (status, code, message) = if error.is_timeout() {
            (StatusCode::GATEWAY_TIMEOUT, "cloud_gateway_timeout", "Cloud model gateway request timed out")
        } else if cause.contains("certificate") || cause.contains("tls") {
            (StatusCode::BAD_GATEWAY, "cloud_gateway_tls", "Cloud model gateway TLS connection failed")
        } else if cause.contains("dns") || cause.contains("resolve") {
            (StatusCode::BAD_GATEWAY, "cloud_gateway_dns", "Cloud model gateway address could not be resolved")
        } else if error.is_connect() {
            (StatusCode::BAD_GATEWAY, "cloud_gateway_connect", "Cloud model gateway connection failed")
        } else {
            (StatusCode::BAD_GATEWAY, "cloud_gateway_transport", "Cloud model gateway transport failed")
        };
        Self { status, code, message }
    }

    pub fn unauthorized() -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "invalid_fusion_token",
            message: "A valid current account Fusion token is required",
        }
    }

    pub fn input(message: &'static str) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_request",
            message,
        }
    }

    pub fn unavailable(message: &'static str) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            code: "local_gateway_unavailable",
            message,
        }
    }
}

impl From<std::io::Error> for GatewayError {
    fn from(_: std::io::Error) -> Self {
        Self::unavailable("Local request storage could not be accessed")
    }
}

impl IntoResponse for GatewayError {
    fn into_response(self) -> Response {
        tracing::warn!(target: "model_gateway", code = self.code,
            status = self.status.as_u16(), reason = self.message, "local model request failed");
        let error_type = if self.status == StatusCode::UNAUTHORIZED {
            "authentication_error"
        } else {
            "local_gateway_error"
        };
        let mut response = (
            self.status,
            axum::Json(serde_json::json!({"error": {
                "type": error_type, "code": self.code, "message": self.message
            }})),
        )
            .into_response();
        if self.status == StatusCode::UNAUTHORIZED {
            response.headers_mut().insert(
                axum::http::header::WWW_AUTHENTICATE,
                axum::http::HeaderValue::from_static("Bearer"),
            );
        }
        response
    }
}
