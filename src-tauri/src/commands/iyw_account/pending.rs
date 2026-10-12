use std::{
    collections::HashMap,
    sync::OnceLock,
    time::{Duration, Instant},
};

use serde_json::Value;

use super::{types::IywAccountToken, wire, AppCommandError};

const FLOW_LIFETIME: Duration = Duration::from_secs(10 * 60);
const MAX_PENDING_FLOWS: usize = 32;
const MAX_CAPTCHA_TICKETS: usize = 64;

struct CaptchaTicket {
    created_at: Instant,
    phone: String,
    kind: i64,
    key: String,
}

fn captchas() -> &'static tokio::sync::Mutex<HashMap<String, CaptchaTicket>> {
    static TICKETS: OnceLock<tokio::sync::Mutex<HashMap<String, CaptchaTicket>>> = OnceLock::new();
    TICKETS.get_or_init(|| tokio::sync::Mutex::new(HashMap::new()))
}

pub(super) async fn remember_captcha(phone: String, kind: i64, key: String) -> String {
    let mut tickets = captchas().lock().await;
    tickets.retain(|_, ticket| ticket.created_at.elapsed() < FLOW_LIFETIME);
    if tickets.len() >= MAX_CAPTCHA_TICKETS {
        if let Some(oldest) = tickets
            .iter()
            .min_by_key(|(_, ticket)| ticket.created_at)
            .map(|(id, _)| id.clone())
        {
            tickets.remove(&oldest);
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    tickets.insert(
        id.clone(),
        CaptchaTicket {
            created_at: Instant::now(),
            phone,
            kind,
            key,
        },
    );
    id
}

pub(super) async fn captcha_for(
    id: &str,
    phone: &str,
    kind: i64,
) -> Result<String, AppCommandError> {
    captchas()
        .lock()
        .await
        .get(id)
        .filter(|ticket| {
            ticket.created_at.elapsed() < FLOW_LIFETIME
                && ticket.phone == phone
                && ticket.kind == kind
        })
        .map(|ticket| ticket.key.clone())
        .ok_or_else(|| {
            AppCommandError::invalid_input("验证码验证已过期或手机号已更换，请重新获取验证码")
        })
}

pub(super) async fn remove_captcha(data: &Value) {
    if let Some(id) = super::types::field(data, "captchaKey") {
        captchas().lock().await.remove(&id);
    }
}
static SCAN_VERSION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn active_scan() -> &'static tokio::sync::Mutex<Option<(String, u64)>> {
    static SCAN: OnceLock<tokio::sync::Mutex<Option<(String, u64)>>> = OnceLock::new();
    SCAN.get_or_init(|| tokio::sync::Mutex::new(None))
}

pub(super) fn begin_scan() -> u64 {
    SCAN_VERSION.fetch_add(1, std::sync::atomic::Ordering::AcqRel) + 1
}

pub(super) fn scan_is_current(version: u64) -> bool {
    SCAN_VERSION.load(std::sync::atomic::Ordering::Acquire) == version
}

pub(super) async fn register_scan(qr: String, version: u64) {
    if scan_is_current(version) {
        *active_scan().lock().await = Some((qr, version));
    }
}

pub(super) async fn scan_version(qr: &str) -> Result<u64, AppCommandError> {
    active_scan()
        .lock()
        .await
        .as_ref()
        .filter(|(current, version)| current == qr && scan_is_current(*version))
        .map(|(_, version)| *version)
        .ok_or_else(|| AppCommandError::invalid_input("二维码已更新，请重新扫码"))
}

pub(super) async fn cancel_scan(qr: &str) {
    let mut scan = active_scan().lock().await;
    if scan
        .as_ref()
        .is_some_and(|(current, version)| current == qr && scan_is_current(*version))
    {
        begin_scan();
        *scan = None;
    }
}

#[derive(Clone)]
pub(super) struct PendingFlow {
    pub created_at: Instant,
    pub token: Option<IywAccountToken>,
    pub wechat_key: Option<String>,
    pub login_data: Value,
    pub workspaces: Vec<Value>,
    pub status: String,
}

impl PendingFlow {
    pub fn new(mut data: Value, status: &str) -> Self {
        if let Some(data) = data.as_object_mut() {
            data.remove("_scanVersion");
        }
        Self {
            created_at: Instant::now(),
            token: super::types::token(&data["token"]),
            wechat_key: super::types::field(&data, "wechatKey"),
            login_data: data,
            workspaces: Vec::new(),
            status: status.to_string(),
        }
    }
}

fn flows() -> &'static tokio::sync::Mutex<HashMap<String, PendingFlow>> {
    static FLOWS: OnceLock<tokio::sync::Mutex<HashMap<String, PendingFlow>>> = OnceLock::new();
    FLOWS.get_or_init(|| tokio::sync::Mutex::new(HashMap::new()))
}

pub(super) async fn put(flow: PendingFlow) -> String {
    let mut flows = flows().lock().await;
    flows.retain(|_, flow| flow.created_at.elapsed() < FLOW_LIFETIME);
    if flows.len() >= MAX_PENDING_FLOWS {
        if let Some(oldest) = flows
            .iter()
            .min_by_key(|(_, flow)| flow.created_at)
            .map(|(id, _)| id.clone())
        {
            flows.remove(&oldest);
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    flows.insert(id.clone(), flow);
    id
}

pub(super) async fn get(data: &Value) -> Result<(String, PendingFlow), AppCommandError> {
    let id = wire::required(data, "flowId")?;
    let mut flows = flows().lock().await;
    flows.retain(|_, flow| flow.created_at.elapsed() < FLOW_LIFETIME);
    let flow = flows
        .get(&id)
        .cloned()
        .ok_or_else(|| AppCommandError::invalid_input("登录验证已过期，请重新登录或扫码"))?;
    Ok((id, flow))
}

pub(super) async fn remove(id: &str) {
    flows().lock().await.remove(id);
}

pub(super) async fn remember_workspace(id: &str, target: Value) {
    if let Some(flow) = flows().lock().await.get_mut(id) {
        flow.workspaces = vec![target];
    }
}

pub(super) async fn clear() {
    flows().lock().await.clear();
    captchas().lock().await.clear();
    super::image_captcha::clear().await;
    begin_scan();
}
