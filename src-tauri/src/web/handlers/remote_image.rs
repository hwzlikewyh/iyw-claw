use axum::body::Body;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use axum::Json;
use serde::Deserialize;

use crate::app_error::AppCommandError;
use crate::commands::remote_image::fetch_remote_image_core;

#[derive(Deserialize)]
pub struct FetchRemoteImageParams {
    pub url: String,
}

pub async fn fetch_remote_image(
    Json(params): Json<FetchRemoteImageParams>,
) -> Result<Response, AppCommandError> {
    let image = fetch_remote_image_core(&params.url).await?;
    let length = image.bytes.len();
    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_static(image.mime_type),
        )
        .header(header::CONTENT_LENGTH, length)
        .header(header::CACHE_CONTROL, "no-store")
        .header("x-content-type-options", "nosniff")
        .body(Body::from(image.bytes))
        .map_err(|error| {
            AppCommandError::task_execution_failed("Cannot build remote image response")
                .with_detail(error.to_string())
        })
}
