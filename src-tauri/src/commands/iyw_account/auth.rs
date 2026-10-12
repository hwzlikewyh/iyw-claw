use sea_orm::DatabaseConnection;
use serde_json::{json, Value};

use super::{auth_forms, login_flow, pending, session, spaces, types::*, wire, AppCommandError};

pub async fn run(
    conn: &DatabaseConnection,
    action: String,
    data: Value,
) -> Result<IywLoginResult, AppCommandError> {
    if action == "cancel" {
        if let Some(qr) = super::types::field(&data, "qrToken") {
            pending::cancel_scan(&qr).await;
        }
    }
    let _guard = session::operation_lock().lock().await;
    match action.as_str() {
        "password" => auth_forms::password(conn, data).await,
        "phone_captcha" => auth_forms::captcha(data).await,
        "image_captcha_refresh" => super::image_captcha::refresh(data).await,
        "image_captcha_verify" => super::image_captcha::verify(data).await,
        "image_captcha_cancel" => super::image_captcha::cancel(data).await,
        "phone" => auth_forms::phone_login(conn, data).await,
        "register" => auth_forms::register(conn, data).await,
        "reset_password" => auth_forms::reset_password(data).await,
        "wechat_poll" => auth_forms::poll(conn, data).await,
        "bind_phone" => auth_forms::bind_phone(conn, data).await,
        "select_workspace" => login_flow::select(conn, data, false).await,
        "create_personal" => login_flow::select(conn, data, true).await,
        "set_identity" => login_flow::identity(conn, data).await,
        "cancel" => {
            if let Ok(id) = wire::required(&data, "flowId") {
                pending::remove(&id).await;
            }
            Ok(IywLoginResult {
                status: "cancelled".into(),
                ..IywLoginResult::default()
            })
        }
        "switch_workspace" => profile_result(spaces::switch(conn, data).await?),
        "recover_workspace" => profile_result(spaces::recover(conn).await?),
        _ => Err(AppCommandError::invalid_input("Unknown account operation")),
    }
}

fn profile_result(profile: IywAccountProfile) -> Result<IywLoginResult, AppCommandError> {
    Ok(IywLoginResult {
        status: "authenticated".into(),
        profile: Some(profile),
        ..IywLoginResult::default()
    })
}

pub(super) async fn qrcode() -> Result<IywWechatQrcode, AppCommandError> {
    let version = pending::begin_scan();
    let response = wire::http_client()?
        .get(format!(
            "{}/api/account-admin/wechat/qrcode?type=login",
            super::ACCOUNT_BASE_URL
        ))
        .header("Referer", format!("{}/", super::ACCOUNT_BASE_URL))
        .send()
        .await
        .map_err(|error| {
            AppCommandError::network("无法加载微信二维码")
                .with_detail(error.without_url().to_string())
        })?;
    let response = wire::decode(response, "wechat_qrcode").await?;
    wire::success(&response, "无法加载微信二维码")?;
    let qr = IywWechatQrcode {
        qrcode_url: wire::required(&response["data"], "qrcodeUrl")?,
        qr_token: wire::required(&response["data"], "qrToken")?,
    };
    pending::register_scan(qr.qr_token.clone(), version).await;
    Ok(qr)
}

pub(super) async fn legacy_poll(
    conn: &DatabaseConnection,
    qr_token: String,
) -> Result<IywWechatPollingResult, AppCommandError> {
    let flow = run(conn, "wechat_poll".into(), json!({"qrToken":qr_token})).await?;
    let status = match flow.status.as_str() {
        "pending" => IywWechatPollingStatus::Pending,
        "authenticated" => IywWechatPollingStatus::Success,
        _ => IywWechatPollingStatus::LoginIncomplete,
    };
    Ok(IywWechatPollingResult {
        status,
        profile: flow.profile.clone(),
        flow: Some(flow),
    })
}
