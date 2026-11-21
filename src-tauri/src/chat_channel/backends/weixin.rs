use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex};

use crate::chat_channel::error::ChatChannelError;
use crate::chat_channel::traits::ChatChannelBackend;
use crate::chat_channel::types::*;
use crate::db::service::sender_context_service;

mod delivery_error;
mod inbound;
mod media;
mod media_crypto;
mod outbox;
mod persistence;
mod polling;
mod protocol;

const ILINK_BASE_URL: &str = "https://ilinkai.weixin.qq.com";
const TYPING_TICKET_TTL: Duration = Duration::from_secs(600);
const TYPING_REQUEST_TIMEOUT: Duration = Duration::from_secs(2);
const TYPING_START: i64 = 1;
const TYPING_STOP: i64 = 2;
const QR_REQUEST_TIMEOUT: Duration = Duration::from_secs(40);
const QR_POLL_HOST_TTL: Duration = Duration::from_secs(10 * 60);
const MAX_QR_POLL_HOSTS: usize = 256;
const INVALID_ARGUMENT_CODE: i64 = -3;

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
    to_user_id: &'a str,
    context_token: &'a str,
    item: serde_json::Value,
    database: &'a DatabaseConnection,
    channel_id: i32,
    reply_context: &'a Mutex<Option<WeixinReplyContext>>,
}

async fn expire_reply_context(req: &SendRequest<'_>) {
    if let Err(error) = sender_context_service::clear_weixin_context_token_if_matches(
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
    if let Some(ref mut context) = *req.reply_context.lock().await {
        if context.to_user_id == req.to_user_id && context.context_token == req.context_token {
            context.expired = true;
        }
    }
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
        .headers(protocol::post_headers(None))
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
    let ret = body.get("ret").and_then(serde_json::Value::as_i64);
    let errcode = body.get("errcode").and_then(serde_json::Value::as_i64);
    ret.filter(|code| *code != 0)
        .or_else(|| errcode.filter(|code| *code != 0))
        .or(ret)
        .or(errcode)
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
    let response = client
        .get(format!("{base_url}/ilink/bot/get_qrcode_status"))
        .headers(protocol::app_headers())
        .query(&query)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status);
    let resp = match response {
        Ok(resp) => resp,
        Err(error)
            if error.is_timeout()
                || error.is_connect()
                || error.status().is_some_and(|s| s.is_server_error()) =>
        {
            return Ok(WeixinQrcodeStatus {
                status: "wait".into(),
                bot_token: None,
                base_url: None,
            });
        }
        Err(error) => {
            return Err(ChatChannelError::ConnectionFailed(weixin_qr_request_error(
                &error,
            )))
        }
    };

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ChatChannelError::ConnectionFailed(format!("QR status parse failed: {e}")))?;

    let status = body
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("wait");
    let status = if status.contains("need_verifycode") {
        "need_verifycode"
    } else {
        status
    }
    .to_owned();

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

#[derive(Clone)]
struct TypingTicket {
    value: String,
    expires_at: Instant,
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
    typing_tickets: Arc<Mutex<HashMap<String, TypingTicket>>>,
}

impl WeixinBackend {
    pub fn new(
        channel_id: i32,
        bot_token: String,
        base_url: String,
        database: DatabaseConnection,
    ) -> Self {
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
            typing_tickets: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn first_updates(&self, cursor: &str) -> Result<serde_json::Value, ChatChannelError> {
        let request = polling::request(&self.client, &self.base_url, &self.bot_token);
        // 验证凭据要收到有效响应，为首次服务端长轮询预留传输时间。
        polling::fetch(
            request,
            cursor,
            polling::DEFAULT_TIMEOUT + Duration::from_secs(10),
        )
        .await?
        .ok_or_else(|| ChatChannelError::ConnectionFailed("Weixin verification timed out".into()))
    }

    /// Build the JSON body for the iLink sendmessage API.
    fn build_send_body(req: &SendRequest<'_>, client_id: &str) -> serde_json::Value {
        serde_json::json!({
            "msg": { "from_user_id": "", "to_user_id": req.to_user_id, "client_id": client_id,
                "message_type": 2, "message_state": 2, "context_token": req.context_token,
                "item_list": [&req.item] },
            "base_info": protocol::base_info()
        })
    }

    /// Send a message via the iLink API and handle the response.
    async fn do_send(req: SendRequest<'_>) -> Result<SentMessageId, ChatChannelError> {
        let client_id = format!("iyw-claw-{}", uuid::Uuid::new_v4());
        let body = Self::build_send_body(&req, &client_id);
        let url = format!("{}/ilink/bot/sendmessage", req.base_url);

        let resp = req
            .client
            .post(&url)
            .headers(protocol::post_headers(Some(req.bot_token)))
            .json(&body)
            .send()
            .await
            .map_err(crate::chat_channel::media_http::transport)?;

        let response = delivery_error::receipt(resp).await?;
        if delivery_error::context_expired(&response) {
            expire_reply_context(&req).await;
        }
        delivery_error::check(&response)?;
        Ok(SentMessageId(
            delivery_error::message_id(&response).unwrap_or(client_id),
        ))
    }

    async fn enqueue_deferred(
        &self,
        to_user_id: &str,
        text: &str,
    ) -> Result<SentMessageId, ChatChannelError> {
        let id = outbox::enqueue(&self.database, self.channel_id, to_user_id, text).await?;
        Err(ChatChannelError::DeliveryDeferred(format!(
            "WAITING_FOR_WECHAT_CONTEXT:{id}"
        )))
    }

    async fn send_or_defer(
        &self,
        to_user_id: &str,
        context_token: &str,
        content: (&str, bool),
    ) -> Result<SentMessageId, ChatChannelError> {
        let (text, managed) = content;
        let result = Self::do_send(SendRequest {
            client: &self.client,
            base_url: &self.base_url,
            bot_token: &self.bot_token,
            to_user_id,
            context_token,
            item: protocol::text_item(text),
            database: &self.database,
            channel_id: self.channel_id,
            reply_context: &self.reply_context,
        })
        .await;
        match result {
            Ok(id) => Ok(id),
            Err(ChatChannelError::DeliveryDeferred(_)) if !managed => {
                self.enqueue_deferred(to_user_id, text).await
            }
            Err(error) => Err(error),
        }
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

        if expired {
            return self.enqueue_deferred(&to_user_id, text).await;
        }

        tracing::info!(
            channel_id = self.channel_id,
            text_chars = text.chars().count(),
            "[Weixin] sending message"
        );

        self.send_or_defer(&to_user_id, &context_token, (text, false))
            .await
    }

    async fn send_text_to(
        &self,
        target: &ChannelMessageTarget,
        text: &str,
    ) -> Result<SentMessageId, ChatChannelError> {
        let to_user_id = target.chat_id.as_deref().ok_or_else(|| {
            ChatChannelError::ConfigurationInvalid("WeChat target user is missing".to_string())
        })?;
        let managed = target
            .provider_payload
            .as_ref()
            .is_some_and(|payload| payload["iyw_managed_delivery"] == true);
        let context_token = self
            .context_token_for(target, to_user_id)
            .await
            .unwrap_or_default();
        if context_token.is_empty() {
            if managed {
                return Err(ChatChannelError::DeliveryDeferred(
                    "TARGET_CONTEXT_UNAVAILABLE".into(),
                ));
            }
            return self.enqueue_deferred(to_user_id, text).await;
        }
        self.send_or_defer(to_user_id, &context_token, (text, managed))
            .await
    }

    async fn context_token_for(
        &self,
        target: &ChannelMessageTarget,
        user_id: &str,
    ) -> Option<String> {
        if let Ok(Some(token)) = sender_context_service::get_weixin_context_token(
            &self.database,
            self.channel_id,
            user_id,
        )
        .await
        {
            if !token.is_empty() {
                return Some(token);
            }
        }
        if let Some(token) = target
            .provider_payload
            .as_ref()
            .and_then(|payload| payload.get("context_token"))
            .and_then(serde_json::Value::as_str)
            .filter(|token| !token.is_empty())
        {
            return Some(token.to_string());
        }
        sender_context_service::get_weixin_context_token(&self.database, self.channel_id, user_id)
            .await
            .ok()
            .flatten()
    }

    async fn typing_ticket(
        &self,
        target: &ChannelMessageTarget,
        user_id: &str,
    ) -> Result<String, ChatChannelError> {
        if let Some(ticket) = self.cached_typing_ticket(user_id).await {
            return Ok(ticket);
        }
        let context_token = self.context_token_for(target, user_id).await;
        let mut body = serde_json::json!({
            "ilink_user_id": user_id,
            "base_info": protocol::base_info(),
        });
        if let Some(token) = context_token {
            body["context_token"] = serde_json::Value::String(token);
        }
        let response = self
            .post_typing_api("getconfig", body)
            .await?
            .json::<serde_json::Value>()
            .await
            .map_err(|_| ChatChannelError::SendFailed("invalid getconfig response".into()))?;
        if response["ret"].as_i64() != Some(0) || response_code(&response) != Some(0) {
            return Err(ChatChannelError::SendFailed(
                "Weixin getconfig did not return success".into(),
            ));
        }
        let ticket = response
            .get("typing_ticket")
            .and_then(serde_json::Value::as_str)
            .filter(|ticket| !ticket.is_empty())
            .ok_or_else(|| ChatChannelError::SendFailed("typing ticket unavailable".into()))?;
        self.typing_tickets.lock().await.insert(
            user_id.to_string(),
            TypingTicket {
                value: ticket.to_string(),
                expires_at: Instant::now() + TYPING_TICKET_TTL,
            },
        );
        Ok(ticket.to_string())
    }

    async fn cached_typing_ticket(&self, user_id: &str) -> Option<String> {
        let mut tickets = self.typing_tickets.lock().await;
        let now = Instant::now();
        tickets.retain(|_, ticket| ticket.expires_at > now);
        tickets.get(user_id).map(|ticket| ticket.value.clone())
    }

    async fn post_typing_api(
        &self,
        endpoint: &str,
        body: serde_json::Value,
    ) -> Result<reqwest::Response, ChatChannelError> {
        let response = self
            .client
            .post(format!("{}/ilink/bot/{endpoint}", self.base_url))
            .headers(protocol::post_headers(Some(&self.bot_token)))
            .timeout(TYPING_REQUEST_TIMEOUT)
            .json(&body)
            .send()
            .await
            .map_err(|error| ChatChannelError::SendFailed(error.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(ChatChannelError::SendFailed(format!("HTTP {status}")));
        }
        Ok(response)
    }

    async fn send_typing_status(
        &self,
        user_id: &str,
        ticket: &str,
        status: i64,
    ) -> Result<(), ChatChannelError> {
        self.post_typing_api(
            "sendtyping",
            serde_json::json!({
                "ilink_user_id": user_id,
                "typing_ticket": ticket,
                "status": status,
                "base_info": protocol::base_info(),
            }),
        )
        .await?;
        Ok(())
    }
}

#[async_trait]
impl ChatChannelBackend for WeixinBackend {
    fn attachment_capability(&self) -> crate::chat_channel::attachments::AttachmentCapability {
        crate::chat_channel::attachments::AttachmentCapability::for_channel("weixin")
    }

    async fn send_attachment_to(
        &self,
        attachment: &crate::chat_channel::attachments::ChannelAttachment,
        target: &ChannelMessageTarget,
    ) -> Result<SentMessageId, ChatChannelError> {
        self.send_media(attachment, target).await
    }

    async fn download_attachment(
        &self,
        attachment: &crate::chat_channel::attachments::IncomingAttachment,
    ) -> Result<crate::chat_channel::attachments::ChannelAttachment, ChatChannelError> {
        inbound::download(attachment).await
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Weixin
    }

    fn supports_typing(&self) -> bool {
        true
    }

    async fn set_typing(
        &self,
        target: &ChannelMessageTarget,
        is_typing: bool,
    ) -> Result<(), ChatChannelError> {
        let user_id = target.chat_id.as_deref().ok_or_else(|| {
            ChatChannelError::ConfigurationInvalid("WeChat target user is missing".to_string())
        })?;
        let ticket = self.typing_ticket(target, user_id).await?;
        let status = if is_typing { TYPING_START } else { TYPING_STOP };
        let result = self.send_typing_status(user_id, &ticket, status).await;
        if result.is_err() {
            let mut tickets = self.typing_tickets.lock().await;
            if tickets
                .get(user_id)
                .is_some_and(|cached| cached.value == ticket)
            {
                tickets.remove(user_id);
            }
        }
        result
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

        let initial_cursor =
            persistence::load_cursor(&self.database, self.channel_id, &self.bot_token)
                .await
                .unwrap_or_default();
        let initial_batch = self.first_updates(&initial_cursor).await?;
        *self.status.lock().await = ChannelConnectionStatus::Connected;

        // Start long-polling loop
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);
        *self.shutdown_tx.lock().await = Some(shutdown_tx);

        let client = self.client.clone();
        let bot_token = self.bot_token.clone();
        let base_url = self.base_url.clone();
        let channel_id = self.channel_id;
        let database = self.database.clone();
        let status = self.status.clone();
        let reply_context = self.reply_context.clone();

        tokio::spawn(async move {
            let mut cursor = initial_cursor;
            let mut first_batch = Some(initial_batch);
            let mut timeout = polling::DEFAULT_TIMEOUT;
            let mut consecutive_errors: u32 = 0;
            while !*shutdown_rx.borrow() {
                let result = if let Some(batch) = first_batch.take() {
                    Ok(Some(batch))
                } else {
                    let request = polling::request(&client, &base_url, &bot_token);
                    tokio::select! {
                        result = polling::fetch(request, &cursor, timeout) => result,
                        _ = shutdown_rx.changed() => break,
                    }
                };
                let body = match result {
                    Ok(Some(body)) => body,
                    Ok(None) => continue,
                    Err(error) => {
                        consecutive_errors = consecutive_errors.saturating_add(1);
                        report_weixin_error(
                            &status,
                            &runtime_tx,
                            channel_id,
                            generation,
                            error.category(),
                            &error.to_string(),
                        )
                        .await;
                        let stopped = if matches!(error, ChatChannelError::AuthenticationFailed(_))
                        {
                            wait_weixin_delay(polling::SESSION_PAUSE, &mut shutdown_rx).await
                        } else {
                            wait_weixin_retry(consecutive_errors, &mut shutdown_rx).await
                        };
                        if stopped {
                            break;
                        }
                        continue;
                    }
                };
                timeout = polling::next_timeout(&body, timeout);
                report_weixin_connected(&status, &runtime_tx, channel_id, generation).await;
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

                        let (text, attachments) = inbound::content(msg);
                        if text.is_empty() {
                            continue;
                        }
                        let text = text.as_str();

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
                            if let Err(error) = sender_context_service::update_weixin_context_token(
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
                            {
                                let mut guard = reply_context.lock().await;
                                *guard = Some(WeixinReplyContext {
                                    to_user_id: from_user_id.to_string(),
                                    context_token: context_token.to_string(),
                                    expired: false,
                                });
                            }
                            outbox::drain(outbox::DrainRequest {
                                client: &client,
                                base_url: &base_url,
                                bot_token: &bot_token,
                                database: &database,
                                channel_id,
                                to_user_id: from_user_id,
                                context_token,
                                reply_context: &reply_context,
                            })
                            .await;
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
                            .and_then(|v| {
                                v.as_str()
                                    .map(str::to_string)
                                    .or_else(|| v.as_u64().map(|id| id.to_string()))
                            })
                            .filter(|v| !v.is_empty())
                            .unwrap_or_else(|| {
                                format!(
                                    "x{}",
                                    weixin_message_hash(
                                        from_user_id,
                                        context_token,
                                        &msg["item_list"].to_string()
                                    )
                                )
                            });
                        let command = IncomingCommand {
                            attachments,
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
                            message_trace_id: super::super::dedupe::new_message_trace_id(
                                channel_id,
                            ),
                            provider_message_id: Some(provider_message_id),
                            received_at: chrono::Utc::now(),
                        };
                        // Bounded queue: never silently drop; reply
                        // busy so the sender retries later.
                        if let Err(send_error) = command_tx.try_send(command) {
                            match send_error {
                                mpsc::error::TrySendError::Full(_) => {
                                    tracing::warn!("[Weixin] dispatcher queue full; replying busy");
                                    let _ = WeixinBackend::do_send(SendRequest {
                                        client: &client,
                                        base_url: &base_url,
                                        bot_token: &bot_token,
                                        to_user_id: from_user_id,
                                        context_token,
                                        item: protocol::text_item(super::DISPATCHER_BUSY_TEXT),
                                        database: &database,
                                        channel_id,
                                        reply_context: &reply_context,
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
                if let Some(next) = body["get_updates_buf"]
                    .as_str()
                    .filter(|next| !next.is_empty())
                {
                    cursor = next.to_owned();
                    if let Err(error) =
                        persistence::save_cursor(&database, channel_id, &bot_token, &cursor).await
                    {
                        tracing::warn!(channel_id, error = %error, "[Weixin] cursor persistence failed");
                    }
                }
            }
            if *shutdown_rx.borrow() {
                *status.lock().await = ChannelConnectionStatus::Disconnected;
            }
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
        self.first_updates("").await.map(|_| ())
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
    let delay = 2u64.saturating_pow(exponent).saturating_mul(5).min(30);
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
