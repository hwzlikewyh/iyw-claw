use sea_orm::DatabaseConnection;
use serde_json::{json, Value};

use super::{pending, types::*, wire, AppCommandError};

pub(super) async fn captcha(data: Value) -> Result<IywLoginResult, AppCommandError> {
    let phone = wire::phone(&data)?;
    let kind = data["type"]
        .as_i64()
        .filter(|kind| matches!(kind, 1 | 3 | 4))
        .ok_or_else(|| AppCommandError::invalid_input("验证码场景无效"))?;
    let response = wire::post(
        "/account-admin/revise/phoneCaptcha",
        json!({"phone":&phone,"type":kind}),
        None,
    )
    .await?;
    wire::success(&response, "发送验证码失败")?;
    let challenge = &response["data"]["processImageCode"];
    let security = challenge.as_bool() == Some(true)
        || scalar(challenge).is_some_and(|flag| flag == "1" || flag == "true");
    if security {
        return super::image_captcha::begin(phone, kind).await;
    }
    let key = field(&response["data"], "captchaKey")
        .ok_or_else(|| AppCommandError::network("验证码发送结果未确认，请稍后检查"))?;
    let captcha_key = Some(pending::remember_captcha(phone, kind, key).await);
    Ok(IywLoginResult {
        status: "captcha".to_string(),
        captcha_key,
        security_verification_required: security,
        ..IywLoginResult::default()
    })
}

pub(super) async fn password(
    conn: &DatabaseConnection,
    data: Value,
) -> Result<IywLoginResult, AppCommandError> {
    let username = wire::required(&data, "username")?;
    let password = data["password"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppCommandError::invalid_input("请输入密码"))?;
    let response = wire::post(
        "/account-admin/login/login",
        json!({"username":username,"password":password,
        "platformType":1,"clientType":1,"deviceType":1}),
        None,
    )
    .await?;
    let result = super::login_flow::handle(conn, response, false).await?;
    pending::remove_captcha(&data).await;
    Ok(result)
}

async fn sms_body(data: &Value, kind: i64) -> Result<Value, AppCommandError> {
    let phone = wire::phone(data)?;
    let captcha = wire::required(data, "captcha")?;
    if captcha.len() != 6 || !captcha.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(AppCommandError::invalid_input("请输入 6 位短信验证码"));
    }
    Ok(
        json!({"phone":phone,"captcha":captcha,"captchaKey":pending::captcha_for(&wire::required(data,"captchaKey")?, &phone, kind).await?,
        "platformType":1,"clientType":1,"deviceType":1}),
    )
}

fn new_password(data: &Value) -> Result<String, AppCommandError> {
    let password = data["password"].as_str().unwrap_or_default();
    let length = password.chars().count();
    let categories = [
        password.chars().any(|ch| ch.is_ascii_alphabetic()),
        password.chars().any(|ch| ch.is_ascii_digit()),
        password.chars().any(|ch| !ch.is_ascii_alphanumeric()),
    ];
    if !(6..=25).contains(&length) || categories.iter().filter(|value| **value).count() < 2 {
        return Err(AppCommandError::invalid_input(
            "密码需为 6–25 位，至少包含两类字符",
        ));
    }
    Ok(password.to_string())
}

pub(super) async fn phone_login(
    conn: &DatabaseConnection,
    data: Value,
) -> Result<IywLoginResult, AppCommandError> {
    let response = wire::post(
        "/account-admin/login/phone",
        sms_body(&data, 1).await?,
        None,
    )
    .await?;
    let result = super::login_flow::handle(conn, response, false).await?;
    pending::remove_captcha(&data).await;
    Ok(result)
}

pub(super) async fn register(
    conn: &DatabaseConnection,
    data: Value,
) -> Result<IywLoginResult, AppCommandError> {
    let mut body = sms_body(&data, 4).await?;
    let fields = body.as_object_mut().expect("sms body is an object");
    fields.insert("password".to_string(), json!(new_password(&data)?));
    for (key, value) in [
        ("market", 0),
        ("registerSource", 1),
        ("sourceType", 3),
        ("isLogin", 1),
    ] {
        fields.insert(key.to_string(), json!(value));
    }
    let mut response = wire::post("/account-admin/revise/phoneSignup", body, None).await?;
    if code(&response) == Some(1) && response["data"]["token"].is_object() {
        response["data"]["selectIntent"] = json!(0);
    }
    let result = super::login_flow::handle(conn, response, false).await?;
    pending::remove_captcha(&data).await;
    Ok(result)
}

pub(super) async fn reset_password(data: Value) -> Result<IywLoginResult, AppCommandError> {
    let sms = sms_body(&data, 3).await?;
    let body = json!({"username":sms["phone"],"captcha":sms["captcha"],"captchaKey":sms["captchaKey"],
        "password":new_password(&data)?,"deviceType":1});
    let response = wire::post("/account-admin/revise/updatePassword", body, None).await?;
    wire::success(&response, "重置密码失败")?;
    pending::remove_captcha(&data).await;
    Ok(IywLoginResult {
        status: "password_reset".to_string(),
        ..IywLoginResult::default()
    })
}

pub(super) async fn bind_phone(
    conn: &DatabaseConnection,
    data: Value,
) -> Result<IywLoginResult, AppCommandError> {
    let (id, flow) = pending::get(&data).await?;
    if flow.status != "phone_binding_required" {
        return Err(AppCommandError::invalid_input("请重新扫码确认微信账号"));
    }
    let mut body = sms_body(&data, 1).await?;
    body.as_object_mut()
        .expect("sms body is an object")
        .remove("platformType");
    body["wechatKey"] = json!(flow
        .wechat_key
        .ok_or_else(|| AppCommandError::invalid_input("请重新扫码"))?);
    let response = wire::post("/account-admin/wechat/loginPhoneBinding", body, None).await?;
    let result = super::login_flow::handle(conn, response, false).await?;
    if result.status != "pending" {
        pending::remove(&id).await;
        pending::remove_captcha(&data).await;
    }
    Ok(result)
}

pub(super) async fn poll(
    conn: &DatabaseConnection,
    data: Value,
) -> Result<IywLoginResult, AppCommandError> {
    let qr_token = wire::required(&data, "qrToken")?;
    let version = pending::scan_version(&qr_token).await?;
    let mut response = wire::post(
        "/account-admin/wechat/polling",
        json!({"qrcodeToken":qr_token,"deviceType":1,"clientType":1}),
        None,
    )
    .await?;
    if !pending::scan_is_current(version) {
        return Err(AppCommandError::invalid_input("二维码已更新，请重新扫码"));
    }
    if response["data"].is_object() {
        response["data"]["_scanVersion"] = json!(version);
    }
    let result = super::login_flow::handle(conn, response, true).await?;
    pending::remove_captcha(&data).await;
    Ok(result)
}
