use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct IywAccountToken {
    pub access_token: String,
    pub refresh_token: String,
    pub expiration: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct IywWorkspace {
    pub org_code: String,
    pub name: String,
    pub logo_url: Option<String>,
    pub org_flag: i32,
    pub enabled: bool,
    pub is_own_create: bool,
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
    pub org_code: Option<String>,
    pub org_list: Vec<IywWorkspace>,
    pub balance_points: Option<f64>,
    pub balance_expiry_time: Option<String>,
    pub points_refresh_failures: u8,
    pub points_stale: bool,
    pub points_requests_paused: bool,
    pub workspace_pending: bool,
    pub business_credentials_pending: bool,
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
    LoginIncomplete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IywWechatPollingResult {
    pub status: IywWechatPollingStatus,
    pub profile: Option<IywAccountProfile>,
    pub flow: Option<IywLoginResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IywImageChallenge {
    pub id: String,
    pub image_data: String,
    pub coords_x: f64,
    pub coords_y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IywLoginResult {
    pub status: String,
    pub profile: Option<IywAccountProfile>,
    pub flow_id: Option<String>,
    pub workspaces: Vec<IywWorkspace>,
    pub message: Option<String>,
    pub captcha_key: Option<String>,
    pub security_verification_required: bool,
    pub image_challenge: Option<IywImageChallenge>,
}

#[derive(Deserialize)]
pub struct IywAccountAuthRequest {
    pub action: String,
    pub data: Value,
}

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub(super) struct StoredSession {
    pub token: Option<IywAccountToken>,
    pub session_id: String,
    pub expires_at_ms: Option<i64>,
    pub org_code: Option<String>,
    pub product_token: Option<String>,
    pub workspace_pending: bool,
    pub business_credentials_pending: bool,
    pub balance: Option<BalanceCache>,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub(super) struct BalanceCache {
    pub scope: String,
    pub amount: Option<f64>,
    pub failures: u8,
}

pub(super) fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(value) if !value.trim().is_empty() => Some(value.trim().to_string()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

pub(super) fn field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(scalar)
}

pub(super) fn code(value: &Value) -> Option<i32> {
    field(value, "code").and_then(|value| value.parse().ok())
}

pub(super) fn token(value: &Value) -> Option<IywAccountToken> {
    Some(IywAccountToken {
        access_token: field(value, "accessToken")?,
        refresh_token: field(value, "refreshToken")?,
        expiration: field(value, "expiration")?,
    })
}

pub(super) fn workspace(value: &Value) -> Option<IywWorkspace> {
    Some(IywWorkspace {
        org_code: field(value, "orgCode")?,
        name: field(value, "name").unwrap_or_default(),
        logo_url: super::normalize_asset_url(field(value, "logo")),
        org_flag: field(value, "orgFlag")
            .and_then(|flag| flag.parse().ok())
            .unwrap_or(1),
        enabled: value.get("status").is_none() || field(value, "status").as_deref() == Some("1"),
        is_own_create: field(value, "isOwnCreate").as_deref() == Some("1"),
    })
}
