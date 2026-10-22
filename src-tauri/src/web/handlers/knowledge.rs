use std::sync::Arc;

use axum::{extract::Extension, Json};
use serde::Deserialize;

use crate::{app_error::AppCommandError, app_state::AppState, commands::knowledge};
use knowledge::models::{BrowseFiles, FilePage, Folder, Selection, UpdateSelection};

#[derive(Deserialize)]
pub struct CategoryParams {
    category: u8,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionParams {
    conversation_id: Option<i32>,
}

#[derive(Deserialize)]
pub struct FilesParams {
    input: BrowseFiles,
}

#[derive(Deserialize)]
pub struct UpdateParams {
    input: UpdateSelection,
}

pub async fn folders(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<CategoryParams>,
) -> Result<Json<Vec<Folder>>, AppCommandError> {
    knowledge::api::KnowledgeClient::new(&state.db.conn)
        .await?
        .folders(params.category)
        .await
        .map(Json)
}

pub async fn files(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<FilesParams>,
) -> Result<Json<FilePage>, AppCommandError> {
    knowledge::api::KnowledgeClient::new(&state.db.conn)
        .await?
        .files(&params.input)
        .await
        .map(Json)
}

pub async fn get_selection(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<SelectionParams>,
) -> Result<Json<Selection>, AppCommandError> {
    knowledge::selection::load(&state.db.conn, params.conversation_id)
        .await
        .map(Json)
}

pub async fn set_selection(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<UpdateParams>,
) -> Result<Json<Selection>, AppCommandError> {
    knowledge::selection::update(&state.db.conn, params.input)
        .await
        .map(Json)
}
