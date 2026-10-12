use std::{
    collections::HashMap,
    sync::OnceLock,
    time::{Duration, Instant},
};

use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};

use super::{pending, types::*, wire, AppCommandError};

const CHALLENGE_LIFETIME: Duration = Duration::from_secs(10 * 60);
const MAX_CHALLENGES: usize = 64;
const MAX_IMAGE_BYTES: usize = 2 * 1024 * 1024;
const MAX_POSITION: f64 = 260.0;
const X_BOUNDS: (f64, f64) = (73.0, 247.0);
const Y_BOUNDS: (f64, f64) = (28.0, 121.0);
const IMAGE_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone)]
struct Challenge {
    created_at: Instant,
    phone: String,
    kind: i64,
    image_url: String,
    key: String,
    x: f64,
    y: f64,
}

fn challenges() -> &'static tokio::sync::Mutex<HashMap<String, Challenge>> {
    static CHALLENGES: OnceLock<tokio::sync::Mutex<HashMap<String, Challenge>>> = OnceLock::new();
    CHALLENGES.get_or_init(|| tokio::sync::Mutex::new(HashMap::new()))
}

pub(super) async fn begin(phone: String, kind: i64) -> Result<IywLoginResult, AppCommandError> {
    let challenge = fetch_challenge(phone, kind).await?;
    let image_data = download_image(&challenge.image_url).await?;
    let id = uuid::Uuid::new_v4().to_string();
    let result = IywImageChallenge {
        id: id.clone(),
        image_data,
        coords_x: challenge.x,
        coords_y: challenge.y,
    };
    remember(id, challenge).await;
    tracing::info!(
        stage = "image_captcha",
        "[iyw-account] SMS safety challenge loaded"
    );
    Ok(IywLoginResult {
        status: "captcha".into(),
        security_verification_required: true,
        image_challenge: Some(result),
        ..IywLoginResult::default()
    })
}

async fn fetch_challenge(phone: String, kind: i64) -> Result<Challenge, AppCommandError> {
    let response = wire::post(
        "/account-admin/login/imageCode",
        json!({"business":&phone,"lowerX":X_BOUNDS.0 as u32,"upperX":X_BOUNDS.1 as u32,
            "lowerY":Y_BOUNDS.0 as u32,"upperY":Y_BOUNDS.1 as u32}),
        None,
    )
    .await?;
    wire::success(&response, "无法加载安全验证，请重试")?;
    let data = &response["data"];
    Ok(Challenge {
        created_at: Instant::now(),
        phone,
        kind,
        image_url: wire::required(data, "imageUrl")?,
        key: wire::required(data, "captchaKey")?,
        x: coordinate(data, "coordsX", X_BOUNDS)?,
        y: coordinate(data, "coordsY", Y_BOUNDS)?,
    })
}

async fn remember(id: String, challenge: Challenge) {
    let mut entries = challenges().lock().await;
    entries.retain(|_, value| value.created_at.elapsed() < CHALLENGE_LIFETIME);
    if entries.len() >= MAX_CHALLENGES {
        if let Some(id) = entries
            .iter()
            .min_by_key(|(_, value)| value.created_at)
            .map(|(id, _)| id.clone())
        {
            entries.remove(&id);
        }
    }
    entries.insert(id, challenge);
}

fn coordinate(data: &Value, name: &str, bounds: (f64, f64)) -> Result<f64, AppCommandError> {
    data[name]
        .as_f64()
        .filter(|value| value.is_finite() && *value >= bounds.0 && *value <= bounds.1)
        .ok_or_else(|| AppCommandError::network("安全验证图片无效，请重新获取"))
}

fn image_url(url: &str) -> Result<reqwest::Url, AppCommandError> {
    let url =
        reqwest::Url::parse(url).map_err(|_| AppCommandError::network("安全验证图片地址无效"))?;
    if url.scheme() != "https"
        || url.host_str() != Some("static.iyw.cn")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port_or_known_default() != Some(443)
        || !url.path().starts_with("/images/graphic/")
    {
        return Err(AppCommandError::network("安全验证图片来源无效"));
    }
    Ok(url)
}

fn image_mime(response: &reqwest::Response) -> Result<String, AppCommandError> {
    let mime = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    if !response.status().is_success()
        || !matches!(mime.as_str(), "image/jpeg" | "image/png" | "image/webp")
    {
        return Err(AppCommandError::network("安全验证图片响应无效，请刷新重试"));
    }
    Ok(mime)
}

async fn download_image(url: &str) -> Result<String, AppCommandError> {
    let url = image_url(url)?;
    let client = reqwest::Client::builder()
        .timeout(IMAGE_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| AppCommandError::network("无法加载安全验证图片"))?;
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|_| AppCommandError::network("安全验证图片加载失败，请刷新重试"))?;
    let mime = image_mime(&response)?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| AppCommandError::network("安全验证图片读取失败"))?
    {
        if bytes.len() + chunk.len() > MAX_IMAGE_BYTES {
            return Err(AppCommandError::network("安全验证图片过大"));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
}

async fn get(data: &Value) -> Result<(String, Challenge), AppCommandError> {
    let id = wire::required(data, "challengeId")?;
    let phone = wire::phone(data)?;
    let entries = challenges().lock().await;
    let challenge = entries
        .get(&id)
        .filter(|value| value.phone == phone && data["type"].as_i64() == Some(value.kind))
        .cloned()
        .ok_or_else(|| {
            AppCommandError::invalid_input("安全验证已过期或手机号已更换，请重新获取验证码")
        })?;
    Ok((id, challenge))
}

pub(super) async fn refresh(data: Value) -> Result<IywLoginResult, AppCommandError> {
    let (id, challenge) = get(&data).await?;
    let next = begin(challenge.phone, challenge.kind).await?;
    challenges().lock().await.remove(&id);
    Ok(next)
}

pub(super) async fn verify(data: Value) -> Result<IywLoginResult, AppCommandError> {
    let (id, challenge) = get(&data).await?;
    if challenge.created_at.elapsed() >= CHALLENGE_LIFETIME {
        return Err(AppCommandError::invalid_input(
            "安全验证已过期，请刷新图片后重试",
        ));
    }
    let x = data["coordsX"]
        .as_f64()
        .filter(|value| {
            value.is_finite() && *value >= 0.0 && *value <= MAX_POSITION && value.fract() == 0.0
        })
        .ok_or_else(|| AppCommandError::invalid_input("滑块位置无效，请重新拖动"))?;
    let response = wire::post(
        "/account-admin/login/imageCode/check",
        json!({
            "type":challenge.kind,"business":challenge.phone,"coordsX":x as u32,
            "imageUrl":challenge.image_url,"coordsY":challenge.y,"captchaKey":challenge.key,
        }),
        None,
    )
    .await?;
    wire::success(&response, "安全验证未通过，请重试或刷新图片")?;
    let key = scalar(&response["data"])
        .or_else(|| field(&response["data"], "captchaKey"))
        .ok_or_else(|| AppCommandError::network("短信发送结果未确认，请稍后检查"))?;
    challenges().lock().await.remove(&id);
    let captcha_key = pending::remember_captcha(challenge.phone, challenge.kind, key).await;
    tracing::info!(
        stage = "image_captcha_verified",
        "[iyw-account] SMS safety check completed"
    );
    Ok(IywLoginResult {
        status: "captcha".into(),
        captcha_key: Some(captcha_key),
        ..IywLoginResult::default()
    })
}

pub(super) async fn cancel(data: Value) -> Result<IywLoginResult, AppCommandError> {
    let id = wire::required(&data, "challengeId")?;
    challenges().lock().await.remove(&id);
    Ok(IywLoginResult {
        status: "cancelled".into(),
        ..IywLoginResult::default()
    })
}

pub(super) async fn clear() {
    challenges().lock().await.clear();
}
