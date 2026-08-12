use axum::body::Body;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use axum::Json;
use serde::Deserialize;

use crate::app_error::AppCommandError;

#[derive(Deserialize)]
pub struct ReadDisplayAssetParams {
    pub hash: String,
}

pub async fn read_display_asset(
    Json(params): Json<ReadDisplayAssetParams>,
) -> Result<Response, AppCommandError> {
    let asset = crate::display_assets::read(params.hash.trim()).await?;
    let length = asset.bytes.len();
    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_static(asset.mime_type),
        )
        .header(header::CONTENT_LENGTH, length)
        .header(
            header::CACHE_CONTROL,
            "private, max-age=31536000, immutable",
        )
        .header("x-content-type-options", "nosniff")
        .body(Body::from(asset.bytes))
        .map_err(|error| {
            AppCommandError::task_execution_failed("Cannot build display image response")
                .with_detail(error.to_string())
        })
}
