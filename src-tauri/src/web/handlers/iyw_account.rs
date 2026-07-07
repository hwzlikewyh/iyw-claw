use std::sync::Arc;

use axum::{extract::Extension, Json};
use serde::Deserialize;

use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::iyw_account::{
    iyw_account_get_profile_core, iyw_account_get_wechat_qrcode_core,
    iyw_account_login_with_password_core, iyw_account_logout_core,
    iyw_account_poll_wechat_login_core, IywAccountProfile, IywWechatPollingResult, IywWechatQrcode,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PollWechatLoginParams {
    pub qr_token: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PasswordLoginParams {
    pub username: String,
    pub password: String,
}

pub async fn get_wechat_qrcode() -> Result<Json<IywWechatQrcode>, AppCommandError> {
    Ok(Json(iyw_account_get_wechat_qrcode_core().await?))
}

pub async fn poll_wechat_login(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<PollWechatLoginParams>,
) -> Result<Json<IywWechatPollingResult>, AppCommandError> {
    Ok(Json(
        iyw_account_poll_wechat_login_core(&state.db.conn, params.qr_token).await?,
    ))
}

pub async fn login_with_password(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<PasswordLoginParams>,
) -> Result<Json<IywAccountProfile>, AppCommandError> {
    Ok(Json(
        iyw_account_login_with_password_core(&state.db.conn, params.username, params.password)
            .await?,
    ))
}

pub async fn get_profile(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<IywAccountProfile>, AppCommandError> {
    Ok(Json(iyw_account_get_profile_core(&state.db.conn).await?))
}

pub async fn logout(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<()>, AppCommandError> {
    iyw_account_logout_core(&state.db.conn).await?;
    Ok(Json(()))
}
