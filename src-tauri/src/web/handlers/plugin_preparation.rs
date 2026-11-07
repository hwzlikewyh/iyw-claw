use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::plugin_preparation::{
    self, ChangeInput, ConnectionView, PreparationView, ReadInput,
};
use axum::{Extension, Json};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct ReadParams {
    input: ReadInput,
}

#[derive(Deserialize)]
pub struct ChangeParams {
    input: ChangeInput,
}

#[derive(Deserialize)]
pub struct ConfigurationParams {
    input: plugin_preparation::ConfigurationInput,
}

#[derive(Deserialize)]
pub struct AccountParams {
    input: plugin_preparation::account::AccountInput,
}

pub async fn account(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<AccountParams>,
) -> Result<Json<crate::plugin_runtime::authentication::AccountStatus>, AppCommandError> {
    plugin_preparation::account::operate(&state.db.conn, params.input, state.emitter.clone())
        .await
        .map(Json)
}

pub async fn configure(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<ConfigurationParams>,
) -> Result<Json<PreparationView>, AppCommandError> {
    plugin_preparation::configuration::save(&state.db.conn, params.input, state.emitter.clone())
        .await
        .map(Json)
}

pub async fn read(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<ReadParams>,
) -> Result<Json<PreparationView>, AppCommandError> {
    plugin_preparation::read(&state.db.conn, params.input)
        .await
        .map(Json)
}

pub async fn approve(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<ChangeParams>,
) -> Result<Json<PreparationView>, AppCommandError> {
    plugin_preparation::change(&state.db.conn, params.input, (true, state.emitter.clone()))
        .await
        .map(Json)
}

pub async fn disable(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<ChangeParams>,
) -> Result<Json<PreparationView>, AppCommandError> {
    plugin_preparation::change(&state.db.conn, params.input, (false, state.emitter.clone()))
        .await
        .map(Json)
}

pub async fn check(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<ChangeParams>,
) -> Result<Json<ConnectionView>, AppCommandError> {
    plugin_preparation::check(&state.db.conn, params.input)
        .await
        .map(Json)
}
