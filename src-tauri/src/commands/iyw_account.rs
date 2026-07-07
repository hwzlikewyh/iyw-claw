use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use std::time::Duration;
#[cfg(feature = "tauri-runtime")]
use tauri::State;

use crate::app_error::AppCommandError;
use crate::db::service::app_metadata_service;
#[cfg(feature = "tauri-runtime")]
use crate::db::AppDatabase;

const IYW_ACCOUNT_SESSION_KEY: &str = "iyw_account_session";
const ACCOUNT_BASE_URL: &str = "https://account.iyw.cn";
const GATEWAY_BASE_URL: &str = "https://gateway.iyw.cn";
const DEFAULT_AVATAR_URL: &str =
    "https://chdesign.oss-cn-shanghai.aliyuncs.com/static/avatar/default.png";
const T34_AUTH_CODE: &str = "T34";
const AUTH_CODES: [&str; 15] = [
    "T33", "T34", "A1", "A2", "T29", "I6", "I1", "I2", "I5", "I8", "I3", "I10", "I11", "S2", "I12",
];

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct IywAccountToken {
    pub access_token: String,
    pub refresh_token: String,
    pub expiration: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct IywAccountProfile {
    pub logged_in: bool,
    pub user_id: Option<String>,
    pub name: Option<String>,
    pub nick_name: Option<String>,
    pub phone: Option<String>,
    pub avatar_url: Option<String>,
    pub org_name: Option<String>,
    pub org_logo_url: Option<String>,
    pub balance_points: Option<i64>,
    pub balance_expiry_time: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IywWechatQrcode {
    pub qrcode_url: String,
    pub qr_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IywWechatPollingStatus {
    Pending,
    Success,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IywWechatPollingResult {
    pub status: IywWechatPollingStatus,
    pub profile: Option<IywAccountProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct StoredSession {
    token: Option<IywAccountToken>,
}

#[derive(Debug, Deserialize)]
struct IywApiResponse<T> {
    code: i32,
    message: Option<String>,
    data: T,
}

#[derive(Debug, Deserialize)]
struct IywApiOptionalResponse<T> {
    code: i32,
    message: Option<String>,
    data: Option<T>,
}

#[derive(Debug, Deserialize)]
struct QrcodeData {
    #[serde(rename = "qrcodeUrl")]
    qrcode_url: String,
    #[serde(rename = "qrToken")]
    qr_token: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PollingRequest<'a> {
    qrcode_token: &'a str,
    device_type: i32,
    client_type: i32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PasswordLoginRequest<'a> {
    username: &'a str,
    password: &'a str,
    platform_type: i32,
    client_type: i32,
    device_type: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExternalToken {
    access_token: String,
    refresh_token: String,
    expiration: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PollingData {
    token: Option<ExternalToken>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PasswordLoginData {
    token: ExternalToken,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MyInfoData {
    user_info: UserInfo,
    org_info: Option<OrgInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserInfo {
    avatar: Option<String>,
    name: Option<String>,
    nick_name: Option<String>,
    user_id: Option<String>,
    phone: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrgInfo {
    name: Option<String>,
    logo: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthListRequest<'a> {
    auth_codes: &'a [&'a str],
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthItem {
    auth_code: String,
    status: i32,
    expiry_time: Option<String>,
    remain: i64,
}

fn http_client() -> Result<reqwest::Client, AppCommandError> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("iyw-claw")
        .build()
        .map_err(|err| {
            AppCommandError::network("Failed to build HTTP client").with_detail(err.to_string())
        })
}

fn account_error(message: Option<String>, fallback: &str) -> AppCommandError {
    AppCommandError::authentication_failed(message.unwrap_or_else(|| fallback.to_string()))
}

fn normalize_asset_url(value: Option<String>) -> Option<String> {
    let value = value?.trim().to_string();
    if value.is_empty() {
        return None;
    }
    if value.starts_with("http://") || value.starts_with("https://") {
        return Some(value);
    }
    if value.starts_with("/saas/") {
        return Some(format!(
            "https://chdesign.oss-cn-shanghai.aliyuncs.com{value}"
        ));
    }
    if value.trim_start_matches('/') == "static/avatar/default.png" {
        return Some(DEFAULT_AVATAR_URL.to_string());
    }
    Some(format!(
        "{ACCOUNT_BASE_URL}/{}",
        value.trim_start_matches('/')
    ))
}

async fn load_session(conn: &DatabaseConnection) -> Result<StoredSession, AppCommandError> {
    let raw = app_metadata_service::get_value(conn, IYW_ACCOUNT_SESSION_KEY)
        .await
        .map_err(AppCommandError::from)?;

    let Some(raw) = raw else {
        return Ok(StoredSession::default());
    };

    serde_json::from_str::<StoredSession>(&raw).map_err(|err| {
        AppCommandError::configuration_invalid("Failed to parse stored iyw account session")
            .with_detail(err.to_string())
    })
}

async fn save_session(
    conn: &DatabaseConnection,
    session: &StoredSession,
) -> Result<(), AppCommandError> {
    let serialized = serde_json::to_string(session).map_err(|err| {
        AppCommandError::invalid_input("Failed to serialize iyw account session")
            .with_detail(err.to_string())
    })?;

    app_metadata_service::upsert_value(conn, IYW_ACCOUNT_SESSION_KEY, &serialized)
        .await
        .map_err(AppCommandError::from)
}

async fn fetch_profile_with_token(token: &str) -> Result<IywAccountProfile, AppCommandError> {
    let client = http_client()?;
    let info_response = client
        .post(format!("{GATEWAY_BASE_URL}/user-service/user/getMyInfo"))
        .header("Accept", "application/json, text/plain, */*")
        .header("Origin", "https://ai.iyw.cn")
        .header("Referer", "https://ai.iyw.cn/")
        .header("token", token)
        .send()
        .await
        .map_err(|err| {
            AppCommandError::network("Failed to load iyw account profile")
                .with_detail(err.to_string())
        })?;

    let info = info_response
        .json::<IywApiResponse<MyInfoData>>()
        .await
        .map_err(|err| {
            AppCommandError::network("Failed to parse iyw account profile")
                .with_detail(err.to_string())
        })?;
    if info.code != 1 {
        return Err(account_error(
            info.message,
            "Failed to load iyw account profile",
        ));
    }

    let mut profile = IywAccountProfile {
        logged_in: true,
        user_id: info.data.user_info.user_id,
        name: info.data.user_info.name,
        nick_name: info.data.user_info.nick_name,
        phone: info.data.user_info.phone,
        avatar_url: normalize_asset_url(info.data.user_info.avatar),
        org_name: info.data.org_info.as_ref().and_then(|org| org.name.clone()),
        org_logo_url: normalize_asset_url(info.data.org_info.and_then(|org| org.logo)),
        balance_points: None,
        balance_expiry_time: None,
    };

    let auth_response = client
        .post(format!(
            "{GATEWAY_BASE_URL}/member/api/v2/MemberAuth/GetAuthList"
        ))
        .header("Accept", "application/json, text/plain, */*")
        .header("Content-Type", "application/json")
        .header("Origin", "https://ai.iyw.cn")
        .header("Referer", "https://ai.iyw.cn/")
        .header("token", token)
        .json(&AuthListRequest {
            auth_codes: &AUTH_CODES,
        })
        .send()
        .await
        .map_err(|err| {
            AppCommandError::network("Failed to load iyw account balance")
                .with_detail(err.to_string())
        })?;

    let auth = auth_response
        .json::<IywApiResponse<Vec<AuthItem>>>()
        .await
        .map_err(|err| {
            AppCommandError::network("Failed to parse iyw account balance")
                .with_detail(err.to_string())
        })?;
    if auth.code == 1 {
        if let Some(item) = auth
            .data
            .into_iter()
            .find(|item| item.auth_code == T34_AUTH_CODE)
        {
            if item.status == 1 {
                profile.balance_points = Some(item.remain);
                profile.balance_expiry_time = item.expiry_time;
            }
        }
    }

    Ok(profile)
}

pub async fn iyw_account_get_wechat_qrcode_core() -> Result<IywWechatQrcode, AppCommandError> {
    let response = http_client()?
        .get(format!(
            "{ACCOUNT_BASE_URL}/api/account-admin/wechat/qrcode?type=login"
        ))
        .header("Accept", "application/json, text/plain, */*")
        .header("Referer", format!("{ACCOUNT_BASE_URL}/"))
        .send()
        .await
        .map_err(|err| {
            AppCommandError::network("Failed to load iyw WeChat qrcode")
                .with_detail(err.to_string())
        })?;

    let body = response
        .json::<IywApiResponse<QrcodeData>>()
        .await
        .map_err(|err| {
            AppCommandError::network("Failed to parse iyw WeChat qrcode")
                .with_detail(err.to_string())
        })?;
    if body.code != 1 {
        return Err(account_error(
            body.message,
            "Failed to load iyw WeChat qrcode",
        ));
    }

    Ok(IywWechatQrcode {
        qrcode_url: body.data.qrcode_url,
        qr_token: body.data.qr_token,
    })
}

pub async fn iyw_account_poll_wechat_login_core(
    conn: &DatabaseConnection,
    qr_token: String,
) -> Result<IywWechatPollingResult, AppCommandError> {
    let trimmed = qr_token.trim();
    if trimmed.is_empty() {
        return Err(AppCommandError::invalid_input("QR token cannot be empty"));
    }

    let response = http_client()?
        .post(format!(
            "{ACCOUNT_BASE_URL}/api/account-admin/wechat/polling"
        ))
        .header("Accept", "application/json, text/plain, */*")
        .header("Content-Type", "application/json")
        .header("Origin", ACCOUNT_BASE_URL)
        .header("Referer", format!("{ACCOUNT_BASE_URL}/"))
        .json(&PollingRequest {
            qrcode_token: trimmed,
            device_type: 1,
            client_type: 1,
        })
        .send()
        .await
        .map_err(|err| {
            AppCommandError::network("Failed to poll iyw WeChat login").with_detail(err.to_string())
        })?;

    let body = response
        .json::<IywApiResponse<PollingData>>()
        .await
        .map_err(|err| {
            AppCommandError::network("Failed to parse iyw WeChat login status")
                .with_detail(err.to_string())
        })?;
    if body.code != 1 {
        return Err(account_error(
            body.message,
            "Failed to poll iyw WeChat login",
        ));
    }

    let Some(token) = body.data.token else {
        return Ok(IywWechatPollingResult {
            status: IywWechatPollingStatus::Pending,
            profile: None,
        });
    };

    let session = StoredSession {
        token: Some(IywAccountToken {
            access_token: token.access_token.clone(),
            refresh_token: token.refresh_token,
            expiration: token.expiration,
        }),
    };
    save_session(conn, &session).await?;
    let profile = fetch_profile_with_token(&token.access_token).await?;

    Ok(IywWechatPollingResult {
        status: IywWechatPollingStatus::Success,
        profile: Some(profile),
    })
}

pub async fn iyw_account_login_with_password_core(
    conn: &DatabaseConnection,
    username: String,
    password: String,
) -> Result<IywAccountProfile, AppCommandError> {
    let username = username.trim();
    if username.is_empty() {
        return Err(AppCommandError::invalid_input("Username cannot be empty"));
    }
    if password.is_empty() {
        return Err(AppCommandError::invalid_input("Password cannot be empty"));
    }

    let response = http_client()?
        .post(format!("{ACCOUNT_BASE_URL}/api/account-admin/login/login"))
        .header("Accept", "application/json, text/plain, */*")
        .header("Content-Type", "application/json")
        .header("Origin", ACCOUNT_BASE_URL)
        .header("Referer", format!("{ACCOUNT_BASE_URL}/"))
        .json(&PasswordLoginRequest {
            username,
            password: &password,
            platform_type: 1,
            client_type: 1,
            device_type: 1,
        })
        .send()
        .await
        .map_err(|err| {
            AppCommandError::network("Failed to sign in to iyw account")
                .with_detail(err.to_string())
        })?;

    let body = response
        .json::<IywApiOptionalResponse<PasswordLoginData>>()
        .await
        .map_err(|err| {
            AppCommandError::network("Failed to parse iyw password login")
                .with_detail(err.to_string())
        })?;
    if body.code != 1 {
        return Err(account_error(
            body.message,
            "Failed to sign in to iyw account",
        ));
    }

    let Some(data) = body.data else {
        return Err(account_error(
            body.message,
            "Failed to sign in to iyw account",
        ));
    };
    let token = data.token;
    let session = StoredSession {
        token: Some(IywAccountToken {
            access_token: token.access_token.clone(),
            refresh_token: token.refresh_token,
            expiration: token.expiration,
        }),
    };
    save_session(conn, &session).await?;
    fetch_profile_with_token(&token.access_token).await
}

pub async fn iyw_account_get_profile_core(
    conn: &DatabaseConnection,
) -> Result<IywAccountProfile, AppCommandError> {
    let session = load_session(conn).await?;
    let Some(token) = session.token else {
        return Ok(IywAccountProfile::default());
    };
    if token.access_token.trim().is_empty() {
        return Ok(IywAccountProfile::default());
    }

    fetch_profile_with_token(&token.access_token).await
}

pub async fn iyw_account_logout_core(conn: &DatabaseConnection) -> Result<(), AppCommandError> {
    save_session(conn, &StoredSession::default()).await
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn iyw_account_get_wechat_qrcode() -> Result<IywWechatQrcode, AppCommandError> {
    iyw_account_get_wechat_qrcode_core().await
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn iyw_account_poll_wechat_login(
    qr_token: String,
    db: State<'_, AppDatabase>,
) -> Result<IywWechatPollingResult, AppCommandError> {
    iyw_account_poll_wechat_login_core(&db.conn, qr_token).await
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn iyw_account_login_with_password(
    username: String,
    password: String,
    db: State<'_, AppDatabase>,
) -> Result<IywAccountProfile, AppCommandError> {
    iyw_account_login_with_password_core(&db.conn, username, password).await
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn iyw_account_get_profile(
    db: State<'_, AppDatabase>,
) -> Result<IywAccountProfile, AppCommandError> {
    iyw_account_get_profile_core(&db.conn).await
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn iyw_account_logout(db: State<'_, AppDatabase>) -> Result<(), AppCommandError> {
    iyw_account_logout_core(&db.conn).await
}
