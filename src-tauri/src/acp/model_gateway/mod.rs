//! 主进程模型出口：只准备媒体并同协议转发，计费与候选选择由 Fusion 完成。
use std::{
    io,
    net::Ipv4Addr,
    sync::{Arc, OnceLock, RwLock},
};

use axum::{routing::any, Router};
use rand::{rngs::OsRng, RngCore};
use tokio::{
    sync::{Mutex, Semaphore},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

mod assets;
mod auth;
mod error;
mod forward;
mod headers;
mod json_image;
mod json_scan;
mod json_string;
mod limits;
mod media;
mod rewrite;
mod spool;
mod spool_budget;
mod target;
mod websocket;
mod work;

pub(crate) use auth::update_credentials;

static ACTIVE_BASE: OnceLock<RwLock<Option<String>>> = OnceLock::new();

fn active_base() -> &'static RwLock<Option<String>> {
    ACTIVE_BASE.get_or_init(|| RwLock::new(None))
}

pub(crate) fn base_url(anthropic: bool) -> Option<String> {
    active_base()
        .read()
        .ok()?
        .as_ref()
        .map(|base| target::model_base(base, anthropic))
}

pub(crate) fn endpoint(path: &str) -> Option<String> {
    active_base()
        .read()
        .ok()?
        .as_ref()
        .map(|base| format!("{base}/v1/{}", path.trim_start_matches('/')))
}

pub(crate) struct ModelGateway {
    shutdown: CancellationToken,
    task: Mutex<Option<JoinHandle<()>>>,
    base: String,
}

pub(super) struct Runtime {
    client: reqwest::Client,
    upstream: String,
    authority: String,
    prefix: String,
    requests: Arc<Semaphore>,
    images: Arc<Semaphore>,
    work: Arc<Semaphore>,
    sockets: Arc<Semaphore>,
    cache: assets::AssetCache,
    asset_locks: Vec<tokio::sync::Mutex<()>>,
    shutdown: CancellationToken,
}

impl ModelGateway {
    pub async fn start() -> io::Result<Self> {
        let tcp = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        crate::web::socket_inherit::mark_listener_non_inheritable(&tcp)?;
        let authority = tcp.local_addr()?.to_string();
        let mut secret = [0_u8; 32];
        OsRng
            .try_fill_bytes(&mut secret)
            .map_err(io::Error::other)?;
        let prefix = format!(
            "/{}",
            secret
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        let shutdown = CancellationToken::new();
        let runtime = Arc::new(Runtime::new(
            authority.clone(),
            prefix.clone(),
            shutdown.clone(),
        )?);
        let base = format!("http://{authority}{prefix}");
        *active_base()
            .write()
            .map_err(|_| io::Error::other("Local gateway state unavailable"))? = Some(base.clone());
        tracing::info!(target: "model_gateway", "process local model gateway ready");
        let task = tokio::spawn(serve(tcp, runtime));
        Ok(Self {
            shutdown,
            task: Mutex::new(Some(task)),
            base,
        })
    }

    pub fn stop(&self) {
        self.shutdown.cancel();
        if let Ok(mut active) = active_base().write() {
            if active.as_deref() == Some(&self.base) {
                *active = None;
            }
        }
    }

    pub async fn shutdown(&self, deadline: tokio::time::Instant) -> bool {
        self.stop();
        let mut task = self.task.lock().await;
        let Some(mut join) = task.take() else {
            return true;
        };
        match tokio::time::timeout_at(deadline, &mut join).await {
            Ok(result) => result.is_ok(),
            Err(_) => {
                join.abort();
                let _ = join.await;
                tracing::warn!(target: "model_gateway", "local model shutdown deadline exceeded");
                false
            }
        }
    }
}

impl Drop for ModelGateway {
    fn drop(&mut self) {
        self.stop();
        if let Some(task) = self.task.get_mut().take() {
            task.abort();
        }
    }
}

impl Runtime {
    fn new(authority: String, prefix: String, shutdown: CancellationToken) -> io::Result<Self> {
        let client = reqwest::Client::builder()
            .connect_timeout(limits::CONNECT_TIMEOUT)
            .read_timeout(limits::HEADER_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .pool_max_idle_per_host(limits::IDLE_CONNECTIONS)
            .build()
            .map_err(io::Error::other)?;
        Ok(Self {
            upstream: crate::acp::provider_overlay::cloud_model_gateway_base_url(),
            client,
            authority,
            prefix,
            shutdown,
            requests: Arc::new(Semaphore::new(limits::MAX_REQUESTS)),
            images: Arc::new(Semaphore::new(limits::MAX_IMAGE_WORK)),
            work: Arc::new(Semaphore::new(limits::MAX_BLOCKING_WORK)),
            sockets: Arc::new(Semaphore::new(limits::MAX_WEBSOCKETS)),
            cache: assets::AssetCache::default(),
            asset_locks: (0..limits::MAX_ASSET_LOCKS)
                .map(|_| tokio::sync::Mutex::new(()))
                .collect(),
        })
    }
}

async fn serve(tcp: tokio::net::TcpListener, runtime: Arc<Runtime>) {
    use futures_util::FutureExt;
    let stop = runtime.shutdown.clone();
    let router = Router::new()
        .fallback(any(forward::handle))
        .with_state(runtime);
    let listener = axum::serve(tcp, router).with_graceful_shutdown(stop.clone().cancelled_owned());
    let result = std::panic::AssertUnwindSafe(async move { listener.await })
        .catch_unwind()
        .await;
    if !matches!(result, Ok(Ok(()))) {
        tracing::error!(target: "model_gateway", "local model listener failed; restart required");
    }
    // 异常退出不回退云端大包；保留地址使请求明确失败，重启主进程服务后重建。
    stop.cancel();
}
