use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use rand::Rng;
use reqwest::header::{HeaderMap, HeaderValue};
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex};

use crate::chat_channel::error::ChatChannelError;
use crate::chat_channel::traits::ChatChannelBackend;
use crate::chat_channel::types::*;
use crate::db::service::sender_context_service;

const ILINK_BASE_URL: &str = "https://ilinkai.weixin.qq.com";
const ILINK_CHANNEL_VERSION: &str = "1.0.2";
const QR_REQUEST_TIMEOUT: Duration = Duration::from_secs(40);
const QR_POLL_HOST_TTL: Duration = Duration::from_secs(10 * 60);
const MAX_QR_POLL_HOSTS: usize = 256;
const INVALID_ARGUMENT_CODE: i64 = -3;
/// Maximum number of messages buffered while context_token is expired.
const MAX_PENDING_MESSAGES: usize = 50;

/// Shared HTTP client for QR code auth requests (avoids re-creating TLS state).
fn qr_client() -> reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(QR_REQUEST_TIMEOUT)
                .build()
                .unwrap_or_default()
        })
        .clone()
}

#[derive(Clone)]
struct QrPollHost {
    base_url: String,
    expires_at: Instant,
}

fn qr_poll_hosts() -> &'static Mutex<HashMap<String, QrPollHost>> {
    static HOSTS: OnceLock<Mutex<HashMap<String, QrPollHost>>> = OnceLock::new();
    HOSTS.get_or_init(|| Mutex::new(HashMap::new()))
}

async fn remember_qr_poll_host(qrcode: &str, base_url: String) {
    let mut hosts = qr_poll_hosts().lock().await;
    prune_qr_poll_hosts(&mut hosts);
    if !hosts.contains_key(qrcode) && hosts.len() >= MAX_QR_POLL_HOSTS {
        evict_earliest_qr_poll_host(&mut hosts);
    }
    hosts.insert(
        qrcode.to_string(),
        QrPollHost {
            base_url,
            expires_at: Instant::now() + QR_POLL_HOST_TTL,
        },
    );
}

async fn qr_poll_base_url(qrcode: &str) -> String {
    let mut hosts = qr_poll_hosts().lock().await;
    prune_qr_poll_hosts(&mut hosts);
    hosts
        .get(qrcode)
        .map(|entry| entry.base_url.clone())
        .unwrap_or_else(|| ILINK_BASE_URL.to_string())
}

fn prune_qr_poll_hosts(hosts: &mut HashMap<String, QrPollHost>) {
    let now = Instant::now();
    hosts.retain(|_, entry| entry.expires_at > now);
}

fn evict_earliest_qr_poll_host(hosts: &mut HashMap<String, QrPollHost>) {
    let earliest = hosts
        .iter()
        .min_by_key(|(_, entry)| entry.expires_at)
        .map(|(qrcode, _)| qrcode.clone());
    if let Some(qrcode) = earliest {
        hosts.remove(&qrcode);
    }
}

// ── QR code auth types (public, used by commands) ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeixinQrcodeInfo {
    pub qrcode_id: String,
    pub qrcode_img_content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeixinQrcodeStatus {
    pub status: String,
    /// bot_token and base_url are consumed by the _core command layer and
    /// stripped before the response reaches the frontend.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
}

/// Frontend-safe subset of [`WeixinQrcodeStatus`] — no credentials.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeixinQrcodeStatusPublic {
    pub status: String,
}

struct SendRequest<'a> {
    client: &'a reqwest::Client,
    base_url: &'a str,
    bot_token: &'a str,
    wechat_uin: &'a str,
    to_user_id: &'a str,
    context_token: &'a str,
    text: &'a str,
    database: &'a DatabaseConnection,
    channel_id: i32,
    reply_context: &'a Mutex<Option<WeixinReplyContext>>,
    pending_messages: &'a Mutex<Vec<String>>,
    allow_buffer: bool,
}

// ── QR code auth functions (called before backend exists) ──

pub async fn weixin_get_qrcode(
    local_token_list: &[String],
) -> Result<WeixinQrcodeInfo, ChatChannelError> {
    let client = qr_client();
    let mut body = request_qrcode(&client, local_token_list).await?;
    if !local_token_list.is_empty() && response_code(&body) == Some(INVALID_ARGUMENT_CODE) {
        tracing::info!(
            local_token_count = local_token_list.len(),
            "[Weixin] saved QR credentials rejected; retrying without local tokens"
        );
        body = request_qrcode(&client, &[]).await?;
    }

    let qrcode_id = body
        .get("qrcode")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let raw_img = body
        .get("qrcode_img_content")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    if qrcode_id.is_empty() {
        return Err(ChatChannelError::ConnectionFailed(
            "Empty qrcode in response".into(),
        ));
    }

    // If the image content is a URL, try to fetch the actual image bytes.
    // If the URL points to an HTML SPA (which renders the QR code via JS),
    // generate the QR code ourselves — the SPA simply encodes the page URL.
    let qrcode_img_content = if raw_img.starts_with("http://") || raw_img.starts_with("https://") {
        match fetch_image_as_data_uri(&client, &raw_img).await {
            Ok(data_uri) => data_uri,
            Err(_) => {
                tracing::info!("[Weixin] URL is an SPA page, generating QR code from URL");
                generate_qrcode_data_uri(&raw_img)?
            }
        }
    } else {
        raw_img
    };

    remember_qr_poll_host(&qrcode_id, ILINK_BASE_URL.to_string()).await;

    Ok(WeixinQrcodeInfo {
        qrcode_id,
        qrcode_img_content,
    })
}

async fn request_qrcode(
    client: &reqwest::Client,
    local_token_list: &[String],
) -> Result<serde_json::Value, ChatChannelError> {
    client
        .post(format!(
            "{ILINK_BASE_URL}/ilink/bot/get_bot_qrcode?bot_type=3"
        ))
        .json(&serde_json::json!({ "local_token_list": local_token_list }))
        .send()
        .await
        .map_err(|error| ChatChannelError::ConnectionFailed(weixin_qr_request_error(&error)))?
        .error_for_status()
        .map_err(|error| ChatChannelError::ConnectionFailed(weixin_qr_request_error(&error)))?
        .json()
        .await
        .map_err(|_| ChatChannelError::ConnectionFailed("QR code response is invalid".into()))
}

fn response_code(body: &serde_json::Value) -> Option<i64> {
    body.get("ret")
        .and_then(serde_json::Value::as_i64)
        .or_else(|| body.get("errcode").and_then(serde_json::Value::as_i64))
}

/// Fetch an image from a URL and return it as a `data:<mime>;base64,...` string.
///
/// Returns an error if the URL points to an HTML page (SPA) rather than a
/// raw image — the caller will generate a QR code from the URL instead.
async fn fetch_image_as_data_uri(
    client: &reqwest::Client,
    url: &str,
) -> Result<String, ChatChannelError> {
    let resp = client
        .get(url)
        .header(
            reqwest::header::USER_AGENT,
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
        )
        .header(reqwest::header::REFERER, ILINK_BASE_URL)
        .send()
        .await
        .map_err(|e| ChatChannelError::ConnectionFailed(format!("Image fetch failed: {e}")))?;

    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("image/png")
        .to_string();

    if content_type.contains("text/html") || content_type.contains("text/plain") {
        return Err(ChatChannelError::ConnectionFailed(
            "QR code URL is an SPA page".into(),
        ));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| ChatChannelError::ConnectionFailed(format!("Image read failed: {e}")))?;

    if bytes.is_empty() {
        return Err(ChatChannelError::ConnectionFailed(
            "Empty image response".into(),
        ));
    }
    let b64 = B64.encode(&bytes);
    let mime = content_type.split(';').next().unwrap_or("image/png").trim();
    Ok(format!("data:{mime};base64,{b64}"))
}

/// Generate a QR code image encoding the given text and return as a PNG data URI.
///
/// The iLink QR page is a SPA that renders `window.location.href` as a QR code.
/// We replicate that logic server-side so the frontend can display it directly.
fn generate_qrcode_data_uri(content: &str) -> Result<String, ChatChannelError> {
    use image::{codecs::png::PngEncoder, ImageEncoder, Luma};
    use qrcode::QrCode;

    let code = QrCode::new(content.as_bytes()).map_err(|e| {
        ChatChannelError::ConnectionFailed(format!("QR code generation failed: {e}"))
    })?;

    let img = code
        .render::<Luma<u8>>()
        .quiet_zone(true)
        .min_dimensions(250, 250)
        .build();
    let (w, h) = (img.width(), img.height());

    let mut png_buf: Vec<u8> = Vec::new();
    PngEncoder::new(&mut png_buf)
        .write_image(img.as_raw(), w, h, image::ExtendedColorType::L8)
        .map_err(|e| ChatChannelError::ConnectionFailed(format!("PNG encoding failed: {e}")))?;

    let b64 = B64.encode(&png_buf);
    Ok(format!("data:image/png;base64,{b64}"))
}

pub async fn weixin_check_qrcode(
    qrcode: &str,
    verify_code: Option<&str>,
) -> Result<WeixinQrcodeStatus, ChatChannelError> {
    let client = qr_client();
    let base_url = qr_poll_base_url(qrcode).await;
    let mut query = vec![("qrcode", qrcode)];
    if let Some(code) = verify_code.map(str::trim).filter(|code| !code.is_empty()) {
        query.push(("verify_code", code));
    }
    let resp = client
        .get(format!("{base_url}/ilink/bot/get_qrcode_status"))
        .query(&query)
        .send()
        .await
        .map_err(|e| ChatChannelError::ConnectionFailed(weixin_qr_request_error(&e)))?
        .error_for_status()
        .map_err(|e| ChatChannelError::ConnectionFailed(weixin_qr_request_error(&e)))?;

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ChatChannelError::ConnectionFailed(format!("QR status parse failed: {e}")))?;

    let status = body
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("waiting")
        .to_string();

    let bot_token = body
        .get("bot_token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let base_url = body
        .get("baseurl")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    if status == "scaned_but_redirect" {
        update_qr_poll_host(qrcode, &body).await;
    }
    if matches!(
        status.as_str(),
        "confirmed" | "expired" | "binded_redirect" | "verify_code_blocked"
    ) {
        qr_poll_hosts().lock().await.remove(qrcode);
    }

    Ok(WeixinQrcodeStatus {
        status,
        bot_token,
        base_url,
    })
}

pub async fn weixin_forget_qrcode(qrcode: &str) {
    qr_poll_hosts().lock().await.remove(qrcode);
}

fn weixin_qr_request_error(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        return "QR status request timed out".to_string();
    }
    if let Some(status) = error.status() {
        return format!("QR status request returned HTTP {status}");
    }
    "QR status request failed".to_string()
}

async fn update_qr_poll_host(qrcode: &str, body: &serde_json::Value) {
    let Some(host) = body
        .get("redirect_host")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|host| !host.is_empty())
    else {
        return;
    };
    let Some(base_url) = trusted_qr_poll_base_url(host) else {
        tracing::warn!("[Weixin] ignored untrusted QR poll redirect host");
        return;
    };
    remember_qr_poll_host(qrcode, base_url).await;
}

fn trusted_qr_poll_base_url(raw_host: &str) -> Option<String> {
    let candidate = if raw_host.contains("://") {
        raw_host.to_string()
    } else {
        format!("https://{raw_host}")
    };
    let parsed = reqwest::Url::parse(&candidate).ok()?;
    let host = parsed
        .host_str()?
        .trim_end_matches('.')
        .to_ascii_lowercase();
    let trusted_host = host == "ilinkai.weixin.qq.com" || host.ends_with(".weixin.qq.com");
    let default_path = parsed.path().is_empty() || parsed.path() == "/";
    if parsed.scheme() != "https"
        || !trusted_host
        || parsed.port_or_known_default() != Some(443)
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || !default_path
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return None;
    }
    Some(format!("https://{host}"))
}

// ── Backend implementation ──

struct WeixinReplyContext {
    to_user_id: String,
    context_token: String,
    expired: bool,
}

pub struct WeixinBackend {
    bot_token: String,
    base_url: String,
    client: reqwest::Client,
    database: DatabaseConnection,
    status: Arc<Mutex<ChannelConnectionStatus>>,
    channel_id: i32,
    shutdown_tx: Arc<Mutex<Option<tokio::sync::watch::Sender<bool>>>>,
    reply_context: Arc<Mutex<Option<WeixinReplyContext>>>,
    /// Messages that failed due to expired context_token, resend on next refresh.
    pending_messages: Arc<Mutex<Vec<String>>>,
    /// Stable X-WECHAT-UIN value for this backend instance.
    wechat_uin: String,
}

impl WeixinBackend {
    pub fn new(
        channel_id: i32,
        bot_token: String,
        base_url: String,
        database: DatabaseConnection,
    ) -> Self {
        let uin_raw = rand::thread_rng().gen::<u32>().to_string();
        let wechat_uin = B64.encode(uin_raw.as_bytes());

        Self {
            bot_token,
            base_url,
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(45))
                .build()
                .unwrap_or_default(),
            database,
            status: Arc::new(Mutex::new(ChannelConnectionStatus::Disconnected)),
            channel_id,
            shutdown_tx: Arc::new(Mutex::new(None)),
            reply_context: Arc::new(Mutex::new(None)),
            pending_messages: Arc::new(Mutex::new(Vec::new())),
            wechat_uin,
        }
    }

    fn build_headers(bot_token: &str, wechat_uin: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("Content-Type", HeaderValue::from_static("application/json"));
        headers.insert(
            "AuthorizationType",
            HeaderValue::from_static("ilink_bot_token"),
        );

        if let Ok(val) = HeaderValue::from_str(wechat_uin) {
            headers.insert("X-WECHAT-UIN", val);
        }

        let bearer = format!("Bearer {bot_token}");
        if let Ok(val) = HeaderValue::from_str(&bearer) {
            headers.insert("Authorization", val);
        }

        headers
    }

    /// Build the JSON body for the iLink sendmessage API.
    fn build_send_body(to_user_id: &str, context_token: &str, text: &str) -> serde_json::Value {
        serde_json::json!({
            "msg": {
                "from_user_id": "",
                "to_user_id": to_user_id,
                "client_id": format!("iyw-claw-{}", uuid::Uuid::new_v4()),
                "message_type": 2,
                "message_state": 2,
                "context_token": context_token,
                "item_list": [{
                    "type": 1,
                    "text_item": { "text": text }
                }]
            },
            "base_info": { "channel_version": ILINK_CHANNEL_VERSION }
        })
    }

    /// Send a message via the iLink API and handle the response.
    /// Returns `Ok(true)` if sent, `Ok(false)` if buffered due to expired context.
    async fn do_send(req: SendRequest<'_>) -> Result<bool, ChatChannelError> {
        let body = Self::build_send_body(req.to_user_id, req.context_token, req.text);
        let url = format!("{}/ilink/bot/sendmessage", req.base_url);

        let resp = req
            .client
            .post(&url)
            .headers(Self::build_headers(req.bot_token, req.wechat_uin))
            .json(&body)
            .send()
            .await
            .map_err(|e| ChatChannelError::SendFailed(e.to_string()))?;

        let status_code = resp.status();
        let resp_text = resp.text().await.unwrap_or_default();

        if !status_code.is_success() {
            return Err(ChatChannelError::SendFailed(format!("HTTP {status_code}")));
        }

        // Check for ret errors in response (e.g. -2 = context expired)
        if let Ok(resp_json) = serde_json::from_str::<serde_json::Value>(&resp_text) {
            if let Some(ret) = resp_json.get("ret").and_then(|v| v.as_i64()) {
                if ret != 0 {
                    tracing::info!(ret, "[Weixin] sendmessage rejected");

                    if ret == -2 {
                        if let Err(error) =
                            sender_context_service::clear_weixin_context_token_if_matches(
                                req.database,
                                req.channel_id,
                                req.to_user_id,
                                req.context_token,
                            )
                            .await
                        {
                            tracing::warn!(
                                channel_id = req.channel_id,
                                error = %error,
                                "[Weixin] failed to clear expired persisted reply context"
                            );
                        }
                        if !req.allow_buffer {
                            return Err(ChatChannelError::SendFailed(
                                "TARGET_CONTEXT_EXPIRED".to_string(),
                            ));
                        }
                        // The implicit reply path may wait for that same
                        // conversation to refresh its context.
                        if let Some(ref mut c) = *req.reply_context.lock().await {
                            c.expired = true;
                        }
                        let mut buf = req.pending_messages.lock().await;
                        if buf.len() < MAX_PENDING_MESSAGES {
                            buf.push(req.text.to_string());
                        }
                        tracing::info!("[Weixin] context_token expired (ret=-2), buffered message");
                        return Ok(false);
                    }

                    return Err(ChatChannelError::SendFailed(format!("provider code {ret}")));
                }
            }
        }

        Ok(true)
    }

    async fn send_text(&self, text: &str) -> Result<SentMessageId, ChatChannelError> {
        // Extract context data under lock, then release
        let (to_user_id, context_token, expired) = {
            let guard = self.reply_context.lock().await;
            let ctx = guard.as_ref().ok_or_else(|| {
                ChatChannelError::SendFailed(
                    "No active WeChat conversation context. A user must message the bot first."
                        .into(),
                )
            })?;
            (
                ctx.to_user_id.clone(),
                ctx.context_token.clone(),
                ctx.expired,
            )
        };

        // If context is expired, buffer the message for resend on next refresh
        if expired {
            tracing::info!(
                "[Weixin] context expired, buffering message (len={})",
                text.len()
            );
            let mut buf = self.pending_messages.lock().await;
            if buf.len() < MAX_PENDING_MESSAGES {
                buf.push(text.to_string());
            } else {
                tracing::info!("[Weixin] pending buffer full, dropping message");
            }
            return Ok(SentMessageId(String::new()));
        }

        tracing::info!(
            channel_id = self.channel_id,
            text_chars = text.chars().count(),
            "[Weixin] sending message"
        );

        Self::do_send(SendRequest {
            client: &self.client,
            base_url: &self.base_url,
            bot_token: &self.bot_token,
            wechat_uin: &self.wechat_uin,
            to_user_id: &to_user_id,
            context_token: &context_token,
            text,
            database: &self.database,
            channel_id: self.channel_id,
            reply_context: &self.reply_context,
            pending_messages: &self.pending_messages,
            allow_buffer: true,
        })
        .await?;

        Ok(SentMessageId(String::new()))
    }

    async fn send_text_to(
        &self,
        target: &ChannelMessageTarget,
        text: &str,
    ) -> Result<SentMessageId, ChatChannelError> {
        let to_user_id = target.chat_id.as_deref().ok_or_else(|| {
            ChatChannelError::ConfigurationInvalid("WeChat target user is missing".to_string())
        })?;
        let target_context_token = target
            .provider_payload
            .as_ref()
            .and_then(|payload| payload.get("context_token"))
            .and_then(serde_json::Value::as_str)
            .filter(|token| !token.is_empty());
        // The in-flight target is the source of truth for a reply. When a
        // backend has restarted, the sender-scoped persisted token restores
        // delivery for targets that only retain the recipient id.
        let context_token = match target_context_token {
            Some(token) => token.to_string(),
            None => sender_context_service::get_weixin_context_token(
                &self.database,
                self.channel_id,
                to_user_id,
            )
            .await
            .map_err(|error| ChatChannelError::SendFailed(error.to_string()))?
            .ok_or_else(|| {
                ChatChannelError::ConfigurationInvalid(
                    "WeChat target context is unavailable".to_string(),
                )
            })?,
        };
        Self::do_send(SendRequest {
            client: &self.client,
            base_url: &self.base_url,
            bot_token: &self.bot_token,
            wechat_uin: &self.wechat_uin,
            to_user_id,
            context_token: &context_token,
            text,
            database: &self.database,
            channel_id: self.channel_id,
            reply_context: &self.reply_context,
            pending_messages: &self.pending_messages,
            allow_buffer: false,
        })
        .await?;
        Ok(SentMessageId(String::new()))
    }
}

#[async_trait]
impl ChatChannelBackend for WeixinBackend {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Weixin
    }

    async fn start(
        &self,
        command_tx: mpsc::Sender<IncomingCommand>,
        runtime_tx: mpsc::Sender<ChannelRuntimeEvent>,
        generation: u64,
    ) -> Result<(), ChatChannelError> {
        *self.status.lock().await = ChannelConnectionStatus::Connecting;

        tracing::info!(
            channel_id = self.channel_id,
            channel_type = "weixin",
            generation,
            stage = "credential_verification",
            "[Weixin] starting backend"
        );

        // Verify auth by doing a quick getupdates with empty cursor
        let verify_body = serde_json::json!({
            "get_updates_buf": "",
            "base_info": { "channel_version": ILINK_CHANNEL_VERSION }
        });
        let url = format!("{}/ilink/bot/getupdates", self.base_url);
        let resp = self
            .client
            .post(&url)
            .headers(Self::build_headers(&self.bot_token, &self.wechat_uin))
            .json(&verify_body)
            .send()
            .await
            .map_err(|e| ChatChannelError::ConnectionFailed(e.to_string()))?;

        let status_code = resp.status();
        let resp_text = resp
            .text()
            .await
            .map_err(|e| ChatChannelError::ConnectionFailed(e.to_string()))?;

        tracing::info!("[Weixin] verify response status={status_code}");

        if !status_code.is_success() {
            return Err(ChatChannelError::ConnectionFailed(format!(
                "Weixin verification returned HTTP {status_code}"
            )));
        }

        let verify_result: serde_json::Value = serde_json::from_str(&resp_text)
            .map_err(|e| ChatChannelError::ConnectionFailed(format!("JSON parse failed: {e}")))?;

        // iLink API auth failures come back as `{"errcode":-14,"errmsg":"session timeout"}`
        // (no `ret` field). Treat any non-zero errcode as authentication failure.
        if let Some(errcode) = verify_result.get("errcode").and_then(|v| v.as_i64()) {
            if errcode != 0 {
                return Err(ChatChannelError::AuthenticationFailed(format!(
                    "Weixin verification failed with provider code {errcode}"
                )));
            }
        }

        let ret = verify_result.get("ret").and_then(|v| v.as_i64());

        // Check for known auth-failure codes
        if ret == Some(-14) {
            return Err(ChatChannelError::AuthenticationFailed(
                "Session expired (ret=-14), please re-authenticate".into(),
            ));
        }

        // The iLink API may omit the `ret` field or return non-zero on the first
        // call. Always extract the cursor if present — it's needed for polling.
        let initial_cursor = verify_result
            .get("get_updates_buf")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if let Some(r) = ret {
            if r != 0 {
                tracing::info!(
                    "[Weixin] verify returned ret={r}, but got cursor len={}",
                    initial_cursor.len()
                );
            }
        }

        *self.status.lock().await = ChannelConnectionStatus::Connected;

        // Start long-polling loop
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);
        *self.shutdown_tx.lock().await = Some(shutdown_tx);

        let client = self.client.clone();
        let bot_token = self.bot_token.clone();
        let base_url = self.base_url.clone();
        let wechat_uin = self.wechat_uin.clone();
        let channel_id = self.channel_id;
        let database = self.database.clone();
        let status = self.status.clone();
        let reply_context = self.reply_context.clone();
        let pending_messages = self.pending_messages.clone();

        tokio::spawn(async move {
            let mut cursor = initial_cursor;
            let mut consecutive_errors: u32 = 0;

            loop {
                if *shutdown_rx.borrow() {
                    break;
                }

                let body = serde_json::json!({
                    "get_updates_buf": cursor,
                    "base_info": { "channel_version": ILINK_CHANNEL_VERSION }
                });

                let result = tokio::select! {
                    r = client
                        .post(format!("{base_url}/ilink/bot/getupdates"))
                        .headers(WeixinBackend::build_headers(&bot_token, &wechat_uin))
                        .json(&body)
                        .send() => r,
                    _ = shutdown_rx.changed() => break,
                };
                if *shutdown_rx.borrow() {
                    break;
                }

                match result {
                    Ok(resp) => {
                        let response_status = resp.status();
                        if !response_status.is_success() {
                            consecutive_errors = consecutive_errors.saturating_add(1);
                            report_weixin_error(
                                &status,
                                &runtime_tx,
                                channel_id,
                                generation,
                                "http_status",
                                &format!("Weixin polling returned HTTP {response_status}"),
                            )
                            .await;
                            if wait_weixin_retry(consecutive_errors, &mut shutdown_rx).await {
                                break;
                            }
                            continue;
                        }

                        if let Ok(body) = resp.json::<serde_json::Value>().await {
                            let ret = body.get("ret").and_then(|v| v.as_i64());
                            let errcode = body.get("errcode").and_then(|v| v.as_i64());

                            // Always update cursor if present
                            if let Some(new_cursor) =
                                body.get("get_updates_buf").and_then(|v| v.as_str())
                            {
                                if !new_cursor.is_empty() {
                                    cursor = new_cursor.to_string();
                                }
                            }

                            // If ret is explicitly non-zero (not just missing), log it
                            if let Some(code) = errcode.filter(|code| *code != 0) {
                                consecutive_errors = consecutive_errors.saturating_add(1);
                                report_weixin_error(
                                    &status,
                                    &runtime_tx,
                                    channel_id,
                                    generation,
                                    "provider_response",
                                    &format!("Weixin polling provider error {code}"),
                                )
                                .await;
                                if wait_weixin_retry(consecutive_errors, &mut shutdown_rx).await {
                                    break;
                                }
                                continue;
                            }
                            if let Some(r) = ret {
                                if r != 0 {
                                    tracing::info!("[Weixin] getupdates ret={r}");
                                }
                                // Session expired — pause and wait for re-auth
                                if r == -14 {
                                    tracing::info!(
                                        "[Weixin] session expired (ret=-14), pausing 30s"
                                    );
                                    report_weixin_error(
                                        &status,
                                        &runtime_tx,
                                        channel_id,
                                        generation,
                                        "authentication",
                                        "Weixin session expired; re-authentication required",
                                    )
                                    .await;
                                    if wait_weixin_delay(Duration::from_secs(30), &mut shutdown_rx)
                                        .await
                                    {
                                        break;
                                    }
                                    continue;
                                }
                                if r != 0 {
                                    consecutive_errors = consecutive_errors.saturating_add(1);
                                    report_weixin_error(
                                        &status,
                                        &runtime_tx,
                                        channel_id,
                                        generation,
                                        "provider_response",
                                        &format!("Weixin polling provider error {r}"),
                                    )
                                    .await;
                                    if wait_weixin_retry(consecutive_errors, &mut shutdown_rx).await
                                    {
                                        break;
                                    }
                                    continue;
                                }
                            }

                            report_weixin_connected(&status, &runtime_tx, channel_id, generation)
                                .await;
                            consecutive_errors = 0;

                            // Process messages
                            if let Some(msgs) = body.get("msgs").and_then(|v| v.as_array()) {
                                if !msgs.is_empty() {
                                    tracing::info!("[Weixin] got {} message(s)", msgs.len());
                                }
                                for msg in msgs {
                                    // Only handle user messages (message_type=1),
                                    // skip bot echo (message_type=2)
                                    let msg_type = msg.get("message_type").and_then(|v| v.as_i64());
                                    if msg_type != Some(1) {
                                        continue;
                                    }

                                    // Extract text from type=1 (text) or type=3 (voice-to-text)
                                    let text = msg
                                        .get("item_list")
                                        .and_then(|v| v.as_array())
                                        .and_then(|items| {
                                            items.iter().find_map(|item| {
                                                let t =
                                                    item.get("type").and_then(|v| v.as_i64())?;
                                                match t {
                                                    1 => item
                                                        .pointer("/text_item/text")
                                                        .and_then(|v| v.as_str()),
                                                    3 => item
                                                        .pointer("/voice_item/text")
                                                        .and_then(|v| v.as_str()),
                                                    _ => None,
                                                }
                                            })
                                        });

                                    let text = match text {
                                        Some(t) if !t.is_empty() => t,
                                        _ => {
                                            tracing::warn!("[Weixin] skipped non-text message");
                                            continue;
                                        }
                                    };

                                    let from_user_id = msg
                                        .get("from_user_id")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or_default();
                                    let context_token = msg
                                        .get("context_token")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or_default();
                                    if from_user_id.is_empty() || context_token.is_empty() {
                                        tracing::warn!(
                                            channel_id,
                                            sender_present = !from_user_id.is_empty(),
                                            context_present = !context_token.is_empty(),
                                            "[Weixin] skipped message without reply context"
                                        );
                                        continue;
                                    }

                                    // Store reply context for outbound messages
                                    // Single lock scope to avoid TOCTOU
                                    if !from_user_id.is_empty() && !context_token.is_empty() {
                                        if let Err(error) =
                                            sender_context_service::update_weixin_context_token(
                                                &database,
                                                channel_id,
                                                from_user_id,
                                                Some(context_token.to_string()),
                                            )
                                            .await
                                        {
                                            tracing::error!(
                                                channel_id,
                                                error = %error,
                                                "[Weixin] failed to persist reply context"
                                            );
                                        }
                                        let was_expired = {
                                            let mut guard = reply_context.lock().await;
                                            let was = guard
                                                .as_ref()
                                                .map(|c| c.to_user_id == from_user_id && c.expired)
                                                .unwrap_or(false);
                                            *guard = Some(WeixinReplyContext {
                                                to_user_id: from_user_id.to_string(),
                                                context_token: context_token.to_string(),
                                                expired: false,
                                            });
                                            was
                                        };

                                        // Resend buffered messages with fresh context
                                        if was_expired {
                                            let buffered: Vec<String> =
                                                pending_messages.lock().await.drain(..).collect();
                                            if !buffered.is_empty() {
                                                tracing::info!(
                                                    "[Weixin] context refreshed, resending {} buffered message(s)",
                                                    buffered.len()
                                                );
                                                for pending_text in &buffered {
                                                    let ok = WeixinBackend::do_send(SendRequest {
                                                        client: &client,
                                                        base_url: &base_url,
                                                        bot_token: &bot_token,
                                                        wechat_uin: &wechat_uin,
                                                        to_user_id: from_user_id,
                                                        context_token,
                                                        text: pending_text,
                                                        database: &database,
                                                        channel_id,
                                                        reply_context: &reply_context,
                                                        pending_messages: &pending_messages,
                                                        allow_buffer: true,
                                                    })
                                                    .await;
                                                    if let Err(e) = ok {
                                                        tracing::error!(
                                                            "[Weixin] resend error: {e}"
                                                        );
                                                        // Re-buffer remaining on hard error
                                                        let mut buf = pending_messages.lock().await;
                                                        if buf.len() < MAX_PENDING_MESSAGES {
                                                            buf.push(pending_text.clone());
                                                        }
                                                    }
                                                    // If do_send returned Ok(false), it
                                                    // already re-buffered internally.
                                                }
                                            }
                                        }
                                    }

                                    tracing::debug!(
                                        channel_id,
                                        content_chars = text.chars().count(),
                                        "[Weixin] dispatching inbound message"
                                    );
                                    // Provider message id: platform id when
                                    // available, else a deterministic composite.
                                    let provider_message_id = msg
                                        .get("msg_id")
                                        .or_else(|| msg.get("message_id"))
                                        .or_else(|| msg.get("client_msg_id"))
                                        .and_then(|v| v.as_str())
                                        .filter(|v| !v.is_empty())
                                        .map(|v| v.to_string())
                                        .unwrap_or_else(|| {
                                            format!(
                                                "x{}",
                                                weixin_message_hash(
                                                    from_user_id,
                                                    context_token,
                                                    text
                                                )
                                            )
                                        });
                                    let command = IncomingCommand {
                                        channel_id,
                                        sender_id: from_user_id.to_string(),
                                        sender_name: None,
                                        command_text: text.to_string(),
                                        callback_data: None,
                                        target: ChannelMessageTarget {
                                            channel_id,
                                            chat_id: Some(from_user_id.to_string()),
                                            thread_key: None,
                                            thread_kind: Some("weixin_context".to_string()),
                                            provider_payload: Some(serde_json::json!({
                                                "context_token": context_token,
                                            })),
                                        },
                                        metadata: serde_json::json!({}),
                                        message_trace_id:
                                            super::super::dedupe::new_message_trace_id(channel_id),
                                        provider_message_id: Some(provider_message_id),
                                        received_at: chrono::Utc::now(),
                                    };
                                    // Bounded queue: never silently drop; reply
                                    // busy so the sender retries later.
                                    if let Err(send_error) = command_tx.try_send(command) {
                                        match send_error {
                                            mpsc::error::TrySendError::Full(_) => {
                                                tracing::warn!(
                                                    "[Weixin] dispatcher queue full; replying busy"
                                                );
                                                let _ = WeixinBackend::do_send(SendRequest {
                                                    client: &client,
                                                    base_url: &base_url,
                                                    bot_token: &bot_token,
                                                    wechat_uin: &wechat_uin,
                                                    to_user_id: from_user_id,
                                                    context_token,
                                                    text: super::DISPATCHER_BUSY_TEXT,
                                                    database: &database,
                                                    channel_id,
                                                    reply_context: &reply_context,
                                                    pending_messages: &pending_messages,
                                                    allow_buffer: true,
                                                })
                                                .await;
                                            }
                                            mpsc::error::TrySendError::Closed(_) => {
                                                tracing::error!(
                                                    "[Weixin] command channel closed; dropping inbound"
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            consecutive_errors = consecutive_errors.saturating_add(1);
                            tracing::error!("[Weixin] failed to parse response body");
                            report_weixin_error(
                                &status,
                                &runtime_tx,
                                channel_id,
                                generation,
                                "decode",
                                "Weixin polling response was not valid JSON",
                            )
                            .await;
                            if wait_weixin_retry(consecutive_errors, &mut shutdown_rx).await {
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        consecutive_errors += 1;
                        tracing::error!("[Weixin] polling error ({consecutive_errors}): {e}");
                        report_weixin_error(
                            &status,
                            &runtime_tx,
                            channel_id,
                            generation,
                            "network",
                            &format!("Weixin polling failed: {e}"),
                        )
                        .await;
                        // Exponential backoff: 5s, 10s, 20s, capped at 30s
                        if wait_weixin_retry(consecutive_errors, &mut shutdown_rx).await {
                            break;
                        }
                    }
                }
            }
            *status.lock().await = ChannelConnectionStatus::Disconnected;
        });

        Ok(())
    }

    async fn stop(&self) -> Result<(), ChatChannelError> {
        if let Some(tx) = self.shutdown_tx.lock().await.take() {
            let _ = tx.send(true);
        }
        *self.status.lock().await = ChannelConnectionStatus::Disconnected;
        Ok(())
    }

    async fn status(&self) -> ChannelConnectionStatus {
        *self.status.lock().await
    }

    async fn send_message(&self, text: &str) -> Result<SentMessageId, ChatChannelError> {
        self.send_text(text).await
    }

    async fn send_rich_message(
        &self,
        message: &RichMessage,
    ) -> Result<SentMessageId, ChatChannelError> {
        let plain_text = message.to_plain_text();
        self.send_text(&plain_text).await
    }

    async fn send_rich_message_to(
        &self,
        message: &RichMessage,
        target: &ChannelMessageTarget,
    ) -> Result<SentMessageId, ChatChannelError> {
        self.send_text_to(target, &message.to_plain_text()).await
    }

    async fn test_connection(&self) -> Result<(), ChatChannelError> {
        let body = serde_json::json!({
            "get_updates_buf": "",
            "base_info": { "channel_version": ILINK_CHANNEL_VERSION }
        });

        let url = format!("{}/ilink/bot/getupdates", self.base_url);
        let resp = self
            .client
            .post(&url)
            .headers(Self::build_headers(&self.bot_token, &self.wechat_uin))
            .json(&body)
            .send()
            .await
            .map_err(|e| ChatChannelError::ConnectionFailed(e.to_string()))?;

        let status_code = resp.status();
        let resp_text = resp
            .text()
            .await
            .map_err(|e| ChatChannelError::ConnectionFailed(e.to_string()))?;

        tracing::info!("[Weixin] test_connection: status={status_code}");

        let resp_json: serde_json::Value = serde_json::from_str(&resp_text)
            .map_err(|e| ChatChannelError::ConnectionFailed(format!("Not valid JSON: {e}")))?;

        if !status_code.is_success() {
            return Err(ChatChannelError::AuthenticationFailed(format!(
                "HTTP {status_code}"
            )));
        }

        // Check for known auth-failure codes
        if let Some(ret) = resp_json.get("ret").and_then(|v| v.as_i64()) {
            if ret == -14 {
                return Err(ChatChannelError::AuthenticationFailed(
                    "Session expired (ret=-14)".into(),
                ));
            }
        }

        Ok(())
    }
}

async fn report_weixin_connected(
    status: &Arc<Mutex<ChannelConnectionStatus>>,
    runtime_tx: &mpsc::Sender<ChannelRuntimeEvent>,
    channel_id: i32,
    generation: u64,
) {
    if !set_weixin_status(status, ChannelConnectionStatus::Connected).await {
        return;
    }
    tracing::info!(
        channel_id,
        channel_type = "weixin",
        generation,
        stage = "getupdates_recovery",
        "[Weixin] polling transport recovered"
    );
    if let Err(error) = runtime_tx
        .send(ChannelRuntimeEvent::Connected {
            channel_id,
            generation,
        })
        .await
    {
        tracing::warn!(
            channel_id,
            generation,
            error = %error,
            "[Weixin] failed to report recovered runtime status"
        );
    }
}

async fn report_weixin_error(
    status: &Arc<Mutex<ChannelConnectionStatus>>,
    runtime_tx: &mpsc::Sender<ChannelRuntimeEvent>,
    channel_id: i32,
    generation: u64,
    error_category: &'static str,
    error: &str,
) {
    if !set_weixin_status(status, ChannelConnectionStatus::Error).await {
        return;
    }
    tracing::warn!(
        channel_id,
        channel_type = "weixin",
        generation,
        stage = "getupdates",
        error_category,
        reason = error,
        "[Weixin] polling transport unavailable"
    );
    if let Err(send_error) = runtime_tx
        .send(ChannelRuntimeEvent::Error {
            channel_id,
            generation,
            error: error.to_string(),
        })
        .await
    {
        tracing::warn!(
            channel_id,
            generation,
            error = %send_error,
            "[Weixin] failed to report runtime error status"
        );
    }
}

async fn set_weixin_status(
    status: &Arc<Mutex<ChannelConnectionStatus>>,
    next: ChannelConnectionStatus,
) -> bool {
    let mut current = status.lock().await;
    if *current == next {
        return false;
    }
    *current = next;
    true
}

async fn wait_weixin_retry(
    consecutive_errors: u32,
    shutdown_rx: &mut tokio::sync::watch::Receiver<bool>,
) -> bool {
    let exponent = consecutive_errors.saturating_sub(1);
    let delay = std::cmp::min(5 * 2u64.saturating_pow(exponent), 30);
    wait_weixin_delay(Duration::from_secs(delay), shutdown_rx).await
}

async fn wait_weixin_delay(
    delay: Duration,
    shutdown_rx: &mut tokio::sync::watch::Receiver<bool>,
) -> bool {
    tokio::select! {
        _ = tokio::time::sleep(delay) => false,
        _ = shutdown_rx.changed() => true,
    }
}

/// Deterministic composite hash for inbound messages that carry no platform
/// message id (used as the idempotency key against duplicate deliveries).
fn weixin_message_hash(from_user_id: &str, context_token: &str, text: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    from_user_id.hash(&mut hasher);
    context_token.hash(&mut hasher);
    text.hash(&mut hasher);
    hasher.finish()
}
