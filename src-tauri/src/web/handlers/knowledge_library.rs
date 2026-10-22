use std::sync::Arc;

use axum::{extract::Extension, Json};
use serde::Deserialize;

use crate::{
    app_error::AppCommandError,
    app_state::AppState,
    commands::knowledge::{library, upload, upload_models::*},
};

#[derive(Deserialize)]
pub struct Category {
    category: u8,
}

#[derive(Deserialize)]
pub struct Input<T> {
    input: T,
}

pub async fn folders(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<Category>,
) -> Result<Json<library::LibraryFolders>, AppCommandError> {
    library::folders(&state.db.conn, params.category)
        .await
        .map(Json)
}

pub async fn files(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<Input<library::LibraryQuery>>,
) -> Result<Json<library::LibraryFiles>, AppCommandError> {
    library::files(&state.db.conn, params.input).await.map(Json)
}

pub async fn refresh(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<Input<library::RefreshFiles>>,
) -> Result<Json<library::RefreshedFiles>, AppCommandError> {
    library::refresh(&state.db.conn, params.input)
        .await
        .map(Json)
}

pub async fn begin(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<Input<BeginUpload>>,
) -> Result<Json<UploadStatus>, AppCommandError> {
    upload::begin(&state.db.conn, params.input).await.map(Json)
}

pub async fn append(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<Input<AppendUpload>>,
) -> Result<Json<UploadStatus>, AppCommandError> {
    upload::append(&state.db.conn, params.input).await.map(Json)
}

pub async fn start(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<Input<UploadRef>>,
) -> Result<Json<UploadStatus>, AppCommandError> {
    upload::start(&state.db.conn, params.input).await.map(Json)
}

pub async fn status(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<Input<UploadRef>>,
) -> Result<Json<UploadStatus>, AppCommandError> {
    upload::status(&state.db.conn, params.input).await.map(Json)
}

pub async fn abort(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<Input<UploadRef>>,
) -> Result<Json<()>, AppCommandError> {
    upload::abort(&state.db.conn, params.input).await.map(Json)
}
