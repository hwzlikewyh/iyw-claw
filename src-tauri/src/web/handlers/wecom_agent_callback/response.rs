use axum::body::Body;
use axum::http::{header, Response, StatusCode};
use axum::response::IntoResponse;

pub(super) fn callback_result(
    result: Result<Response<Body>, CallbackError>,
    channel_id: i32,
    request_id: uuid::Uuid,
    stage: &'static str,
) -> Response<Body> {
    match result {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!(
                channel_id,
                %request_id,
                stage,
                error_code = error.code(),
                "[WeCom Agent] callback rejected"
            );
            empty_response(error.status())
        }
    }
}

pub(super) fn empty_response(status: StatusCode) -> Response<Body> {
    (status, "").into_response()
}

pub(super) fn text_response(status: StatusCode, body: String) -> Response<Body> {
    (
        status,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        body,
    )
        .into_response()
}

pub(super) fn xml_response(status: StatusCode, body: String) -> Response<Body> {
    (
        status,
        [(header::CONTENT_TYPE, "application/xml; charset=utf-8")],
        body,
    )
        .into_response()
}

#[derive(Debug)]
pub(super) enum CallbackError {
    NotFound,
    BadRequest,
    Unauthorized,
    Unavailable,
    Internal,
}

impl CallbackError {
    fn status(&self) -> StatusCode {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::BadRequest => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::NotFound => "CALLBACK_NOT_FOUND",
            Self::BadRequest => "CALLBACK_INVALID",
            Self::Unauthorized => "CALLBACK_UNAUTHORIZED",
            Self::Unavailable => "DISPATCHER_UNAVAILABLE",
            Self::Internal => "CALLBACK_INTERNAL",
        }
    }
}
