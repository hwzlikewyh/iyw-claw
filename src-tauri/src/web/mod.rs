pub mod auth;
pub mod event_bridge;
pub mod handlers;
pub mod port_probe;
pub mod router;
pub mod shutdown;
pub mod socket_inherit;
pub mod static_assets;
pub mod ws;
pub mod ws_attach;

pub use port_probe::{PortState, WebServicePortProbe};

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex};

use shutdown::ShutdownSignal;

use sea_orm::{DatabaseConnection, TransactionError, TransactionTrait};
use serde::{Deserialize, Serialize};

use crate::app_error::{AppCommandError, AppErrorCode};
use crate::app_state::AppState;
use crate::db::service::app_metadata_service;

const WEB_SERVICE_TOKEN_KEY: &str = "web_service_token";
const WEB_SERVICE_PORT_KEY: &str = "web_service_port";
const WEB_SERVICE_AUTO_START_KEY: &str = "web_service_auto_start";
pub const DEFAULT_WEB_SERVICE_PORT: u16 = 3080;

pub struct WebServerState {
    handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
    shutdown_tx: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    /// Coordinates shutdown of live WebSocket handlers. Sticky flag +
    /// `Notify` together: existing handlers wake immediately, and any
    /// handshake completing during the stop window also exits without
    /// leaking an orphan task. Reused across stop→start cycles via
    /// `reset()` at the start of every successful bind.
    pub(crate) shutdown_signal: Arc<ShutdownSignal>,
    port: AtomicU16,
    token: Mutex<String>,
    /// Address the listener is bound to (`0.0.0.0` for a wildcard bind).
    /// Lets `get_web_server_status` advertise only reachable addresses: a
    /// specific bind makes the other interfaces' IPs unreachable.
    host: Mutex<String>,
    running: std::sync::atomic::AtomicBool,
}

impl Default for WebServerState {
    fn default() -> Self {
        Self::new()
    }
}

impl WebServerState {
    pub fn new() -> Self {
        Self {
            handle: Mutex::new(None),
            shutdown_tx: Mutex::new(None),
            shutdown_signal: Arc::new(ShutdownSignal::new()),
            port: AtomicU16::new(0),
            token: Mutex::new(String::new()),
            host: Mutex::new("0.0.0.0".to_string()),
            running: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Handle to the shutdown coordinator. Exposed so binaries / external
    /// callers (e.g. `iyw-claw-server`) can pass it to `build_router`.
    pub fn shutdown_signal(&self) -> Arc<ShutdownSignal> {
        self.shutdown_signal.clone()
    }

    /// Mark the server as running from outside the Tauri command path.
    /// `iyw-claw-server` calls `axum::serve` directly without going through
    /// `start_web_server`, so without this the `running` flag stays
    /// `false` and `get_web_server_status` lies to web-mode browsers.
    /// Note: handle/shutdown_tx are intentionally left `None` — the bin
    /// owns the serve task itself, not this state. `stop_web_server`
    /// uses that absence to detect web mode and reject the call.
    pub fn mark_externally_running(&self, host: String, port: u16, token: String) {
        self.port.store(port, Ordering::Relaxed);
        *self.token.lock().unwrap() = token;
        *self.host.lock().unwrap() = host;
        self.running.store(true, Ordering::Release);
    }

    /// True when the serve task is owned externally (e.g. by `iyw-claw-server`
    /// `axum::serve` in standalone mode), in which case stop/start through
    /// this state must be a no-op.
    pub fn is_externally_managed(&self) -> bool {
        self.handle.lock().unwrap().is_none() && self.running.load(Ordering::Acquire)
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebServerInfo {
    pub port: u16,
    pub token: String,
    pub addresses: Vec<String>,
}

pub fn generate_random_token() -> String {
    uuid::Uuid::new_v4().to_string().replace('-', "")
}

/// Resolve the token to use when starting the Web server:
/// 1. use the explicit override if non-empty;
/// 2. fall back to the persisted value in `AppMetadata`;
/// 3. otherwise generate a fresh random token.
async fn resolve_web_service_token(
    conn: &DatabaseConnection,
    override_token: Option<String>,
) -> Result<String, AppCommandError> {
    let trimmed_override = override_token
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    if let Some(value) = trimmed_override {
        return Ok(value);
    }

    match app_metadata_service::get_value(conn, WEB_SERVICE_TOKEN_KEY)
        .await
        .map_err(AppCommandError::from)?
    {
        Some(saved) if !saved.trim().is_empty() => Ok(saved),
        _ => Ok(generate_random_token()),
    }
}

/// Resolve the access token for the **standalone** server, persisting a
/// generated one so it survives restarts. This matters for self-update: the
/// upgrade restarts the process, and if the token rotated, the already
/// authenticated frontend would start getting 401s and could no longer tell a
/// successful upgrade from an auto-rollback. Resolution mirrors the desktop web
/// service — a non-empty `IYW_CLAW_TOKEN` override wins; otherwise reuse the value
/// persisted in `AppMetadata`; otherwise generate one and persist it. An empty
/// or whitespace override is treated as unset (never accepted as a real token).
/// `*generated` is set when a fresh token was created, so the caller can show
/// it to the operator.
pub async fn resolve_persisted_server_token(
    conn: &DatabaseConnection,
    override_token: Option<String>,
    generated: &mut bool,
) -> String {
    *generated = false;

    if let Some(value) = override_token
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        return value;
    }

    if let Ok(Some(saved)) = app_metadata_service::get_value(conn, WEB_SERVICE_TOKEN_KEY).await {
        if !saved.trim().is_empty() {
            return saved;
        }
    }

    let token = generate_random_token();
    *generated = true;
    if let Err(e) = app_metadata_service::upsert_value(conn, WEB_SERVICE_TOKEN_KEY, &token).await {
        tracing::warn!(
            "[SERVER][WARN] could not persist the generated access token ({e}); it will rotate on \
             restart and self-update success detection may be unreliable — set IYW_CLAW_TOKEN to pin it"
        );
    }
    token
}

pub async fn persisted_server_token(conn: &DatabaseConnection) -> Option<String> {
    app_metadata_service::get_value(conn, WEB_SERVICE_TOKEN_KEY)
        .await
        .ok()
        .flatten()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

async fn resolve_web_service_port(
    conn: &DatabaseConnection,
    override_port: Option<u16>,
) -> Result<u16, AppCommandError> {
    if let Some(port) = override_port {
        return Ok(port);
    }
    let saved = app_metadata_service::get_value(conn, WEB_SERVICE_PORT_KEY)
        .await
        .map_err(AppCommandError::from)?;
    let port = saved
        .as_deref()
        .and_then(|s| s.trim().parse::<u16>().ok())
        .unwrap_or(DEFAULT_WEB_SERVICE_PORT);
    Ok(port)
}

/// Persist token and port atomically so a partial failure cannot leave
/// `app_metadata` in a mixed old/new state.
async fn persist_web_service_config(
    conn: &DatabaseConnection,
    token: &str,
    port: u16,
) -> Result<(), AppCommandError> {
    // Own the values so the inner future is 'static (required by transaction).
    let token_owned = token.to_string();
    let port_str = port.to_string();
    conn.transaction::<_, (), AppCommandError>(move |txn| {
        Box::pin(async move {
            app_metadata_service::upsert_value(txn, WEB_SERVICE_TOKEN_KEY, &token_owned)
                .await
                .map_err(AppCommandError::from)?;
            app_metadata_service::upsert_value(txn, WEB_SERVICE_PORT_KEY, &port_str)
                .await
                .map_err(AppCommandError::from)?;
            Ok(())
        })
    })
    .await
    .map_err(|e: TransactionError<AppCommandError>| match e {
        TransactionError::Connection(db) => {
            AppCommandError::new(AppErrorCode::DatabaseError, "Database transaction failed")
                .with_detail(db.to_string())
        }
        TransactionError::Transaction(inner) => inner,
    })
}

fn parse_bool_metadata(value: Option<String>) -> bool {
    matches!(
        value.as_deref().map(str::trim),
        Some("true") | Some("1") | Some("yes")
    )
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebServiceConfig {
    pub token: Option<String>,
    pub port: Option<u16>,
    pub auto_start: bool,
}

pub async fn load_web_service_config(
    conn: &DatabaseConnection,
) -> Result<WebServiceConfig, AppCommandError> {
    let token = app_metadata_service::get_value(conn, WEB_SERVICE_TOKEN_KEY)
        .await
        .map_err(AppCommandError::from)?;
    let port = app_metadata_service::get_value(conn, WEB_SERVICE_PORT_KEY)
        .await
        .map_err(AppCommandError::from)?
        .as_deref()
        .and_then(|s| s.trim().parse::<u16>().ok());
    let auto_start = parse_bool_metadata(
        app_metadata_service::get_value(conn, WEB_SERVICE_AUTO_START_KEY)
            .await
            .map_err(AppCommandError::from)?,
    );
    Ok(WebServiceConfig {
        token: token.filter(|value| !value.trim().is_empty()),
        port,
        auto_start,
    })
}

pub async fn update_web_service_config_core(
    conn: &DatabaseConnection,
    config: WebServiceConfig,
) -> Result<WebServiceConfig, AppCommandError> {
    let token = config.token.unwrap_or_default().trim().to_string();
    let port = config.port.unwrap_or(DEFAULT_WEB_SERVICE_PORT);
    let auto_start = if config.auto_start { "true" } else { "false" }.to_string();
    let port_str = port.to_string();

    conn.transaction::<_, (), AppCommandError>(move |txn| {
        Box::pin(async move {
            app_metadata_service::upsert_value(txn, WEB_SERVICE_TOKEN_KEY, &token)
                .await
                .map_err(AppCommandError::from)?;
            app_metadata_service::upsert_value(txn, WEB_SERVICE_PORT_KEY, &port_str)
                .await
                .map_err(AppCommandError::from)?;
            app_metadata_service::upsert_value(txn, WEB_SERVICE_AUTO_START_KEY, &auto_start)
                .await
                .map_err(AppCommandError::from)?;
            Ok(())
        })
    })
    .await
    .map_err(|e: TransactionError<AppCommandError>| match e {
        TransactionError::Connection(db) => {
            AppCommandError::new(AppErrorCode::DatabaseError, "Database transaction failed")
                .with_detail(db.to_string())
        }
        TransactionError::Transaction(inner) => inner,
    })?;

    load_web_service_config(conn).await
}

/// Stable i18n-key prefixes — the frontend maps these to localized text.
const ERR_ALREADY_RUNNING: &str = "web_server.already_running";
const ERR_INVALID_ADDRESS: &str = "web_server.invalid_address";
const ERR_PORT_IN_USE: &str = "web_server.port_in_use";
const ERR_PERMISSION_DENIED: &str = "web_server.permission_denied";
const ERR_ADDRESS_UNAVAILABLE: &str = "web_server.address_unavailable";
const ERR_BIND_FAILED: &str = "web_server.bind_failed";

fn classify_bind_error(err: std::io::Error) -> AppCommandError {
    use std::io::ErrorKind;
    let (code, key) = match err.kind() {
        ErrorKind::AddrInUse => (AppErrorCode::AlreadyExists, ERR_PORT_IN_USE),
        ErrorKind::PermissionDenied => (AppErrorCode::PermissionDenied, ERR_PERMISSION_DENIED),
        ErrorKind::AddrNotAvailable => (AppErrorCode::InvalidInput, ERR_ADDRESS_UNAVAILABLE),
        _ => (AppErrorCode::IoError, ERR_BIND_FAILED),
    };
    AppCommandError::new(code, key).with_detail(err.to_string())
}

pub(crate) fn find_static_dir_fallback() -> PathBuf {
    // Dev mode: "out/" is at the project root, which is one level above src-tauri/.
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let project_out = manifest_dir.parent().map(|p| p.join("out"));
    if let Some(ref out) = project_out {
        if out.join("index.html").exists() {
            tracing::info!(
                "[WEB] Serving static files from project out/: {}",
                out.display()
            );
            return out.clone();
        }
    }

    // Fallback: current working directory / out
    let cwd_out = std::env::current_dir()
        .map(|d| d.join("out"))
        .unwrap_or_else(|_| PathBuf::from("out"));
    tracing::warn!(
        "[WEB] Fallback static dir (may not exist): {}",
        cwd_out.display()
    );
    cwd_out
}

pub fn find_static_dir_standalone(explicit: Option<&str>) -> PathBuf {
    if let Some(dir) = explicit {
        let p = PathBuf::from(dir);
        if p.join("index.html").exists() {
            tracing::info!(
                "[WEB] Serving static files from IYW_CLAW_STATIC_DIR: {}",
                p.display()
            );
            return p;
        }
    }

    // Try ./web/
    let web = PathBuf::from("web");
    if web.join("index.html").exists() {
        tracing::info!("[WEB] Serving static files from ./web/: {}", web.display());
        return web;
    }

    find_static_dir_fallback()
}

/// RAII guard that resets `running` back to `false` on drop unless disarmed.
/// Used to guarantee the flag is released on any error during start.
struct RunningGuard<'a> {
    running: &'a std::sync::atomic::AtomicBool,
    armed: bool,
}

impl<'a> RunningGuard<'a> {
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl<'a> Drop for RunningGuard<'a> {
    fn drop(&mut self) {
        if self.armed {
            self.running.store(false, Ordering::Release);
        }
    }
}

/// Whether a local IPv4 is a useful, reachable target worth advertising.
///
/// Rejects loopback (added separately and always listed first), link-local
/// (169.254.0.0/16, not routable for sharing), and the unspecified address
/// (`0.0.0.0` is the bind sentinel, never a reachable target). Applied to
/// both the interface-enumeration and the UDP-probe fallback paths so the
/// advertised list upholds the same invariant regardless of which produced
/// it.
fn is_advertisable_ipv4(ip: std::net::Ipv4Addr) -> bool {
    !ip.is_loopback() && !ip.is_link_local() && !ip.is_unspecified()
}

/// Build the list of candidate URLs to advertise for the running web
/// service. Loopback is always listed first (a safe default target), then
/// every advertisable local IPv4 (see [`is_advertisable_ipv4`]).
///
/// In the desktop settings flow the listener binds `0.0.0.0`, so each
/// advertised address is reachable and the UI lets the user pick which one
/// to display / open — that choice is display-only and never changes what
/// the service binds to. If interface enumeration is unavailable we fall
/// back to the default-route UDP probe so the result never regresses below
/// the previous single-LAN-IP behavior.
pub fn get_local_addresses(port: u16) -> Vec<String> {
    use std::net::{IpAddr, Ipv4Addr};

    let mut lan: Vec<Ipv4Addr> = Vec::new();
    if let Ok(interfaces) = if_addrs::get_if_addrs() {
        for iface in interfaces {
            if let IpAddr::V4(ip) = iface.ip() {
                if is_advertisable_ipv4(ip) && !lan.contains(&ip) {
                    lan.push(ip);
                }
            }
        }
    }

    // Fallback: derive the default-route source IP via the UDP-connect
    // trick when enumeration yields nothing (restricted sandboxes, etc.).
    // Same advertisability filter as above keeps the invariant intact.
    if lan.is_empty() {
        if let Ok(sock) = std::net::UdpSocket::bind("0.0.0.0:0") {
            if sock.connect("8.8.8.8:80").is_ok() {
                if let Ok(local_addr) = sock.local_addr() {
                    if let IpAddr::V4(ip) = local_addr.ip() {
                        if is_advertisable_ipv4(ip) {
                            lan.push(ip);
                        }
                    }
                }
            }
        }
    }

    lan.sort_unstable();

    let mut addrs = vec![format!("http://127.0.0.1:{}", port)];
    addrs.extend(lan.into_iter().map(|ip| format!("http://{}:{}", ip, port)));
    addrs
}

/// Normalize the host to advertise / store for a freshly bound listener.
///
/// Depending on the runtime, the configured host may not be a bare IP
/// literal: the standalone `iyw-claw-server` binds via `ToSocketAddrs`, so it
/// also accepts `localhost` (DNS-resolved) and bracketed IPv6 (`[::1]`),
/// whereas the desktop/web cores parse a `SocketAddr` and accept only IP
/// literals (bare or bracketed IPv6, never a hostname). [`addresses_for_bind`]
/// reasons over bare IPs, so preferring the listener's effective
/// `local_addr()` IP collapses every accepted form to the concrete bound IP
/// (`localhost` → its resolved loopback IP, e.g. `127.0.0.1` or `::1`;
/// `[::1]` → `::1`); the advertised list then always matches what the socket
/// is actually serving. Falls back to the
/// configured host only when `local_addr()` is unavailable (near-impossible
/// for a bound listener).
pub fn advertise_host(local_addr: Option<std::net::SocketAddr>, configured_host: &str) -> String {
    local_addr
        .map(|addr| addr.ip().to_string())
        .unwrap_or_else(|| configured_host.to_string())
}

/// Addresses to advertise for a server bound to `host` (a bare IP — see
/// [`advertise_host`], which normalizes the configured host into one).
///
/// A specific (non-wildcard) bind address is the *only* reachable target,
/// so advertise just that. A wildcard bind (`0.0.0.0` / `::`, or an
/// unparseable value) serves every interface, so fall back to enumerating
/// loopback + all local IPv4 via [`get_local_addresses`].
pub fn addresses_for_bind(host: &str, port: u16) -> Vec<String> {
    // Tolerate a bracketed IPv6 literal (`[::1]`): `IpAddr`'s parser rejects
    // brackets, but the configured-host fallback (when `local_addr()` was
    // unavailable) may still carry them.
    let host = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        if !ip.is_unspecified() {
            // `SocketAddr`'s Display brackets IPv6 (`[::1]:port`) so the URL
            // is well-formed for both families; a bare `format!("{ip}:{port}")`
            // would emit the invalid `http://::1:port` for IPv6.
            return vec![format!("http://{}", std::net::SocketAddr::new(ip, port))];
        }
    }
    get_local_addresses(port)
}

// ── Core logic (shared by Tauri commands and web handlers) ──

#[allow(dead_code)]
pub(crate) async fn do_start_web_server_with_state(
    app_state: Arc<AppState>,
    static_dir: PathBuf,
    port: Option<u16>,
    host: Option<String>,
    token: Option<String>,
) -> Result<WebServerInfo, AppCommandError> {
    let ws = &app_state.web_server_state;

    // Atomically claim the running flag; concurrent starts see AlreadyExists.
    ws.running
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| AppCommandError::new(AppErrorCode::AlreadyExists, ERR_ALREADY_RUNNING))?;
    let mut guard = RunningGuard {
        running: &ws.running,
        armed: true,
    };

    let port = resolve_web_service_port(&app_state.db.conn, port).await?;
    let host = host.unwrap_or_else(|| "0.0.0.0".to_string());
    let token = resolve_web_service_token(&app_state.db.conn, token).await?;

    // Validate the upload-quota strict-mode posture before any I/O. A
    // misconfigured env var must surface as a clean `AppCommandError`
    // on the desktop — the standalone server's process-exit path is
    // wrong here because that would take down the whole webview app
    // (and the persisted web-service config would survive the crash,
    // re-tripping on every relaunch).
    handlers::files::log_upload_quota_config_at_startup();
    if let Err(err) = handlers::files::validate_upload_quota_config() {
        return Err(AppCommandError::new(
            AppErrorCode::InvalidInput,
            "Upload quota configuration is invalid",
        )
        .with_detail(err.to_string()));
    }

    let addr: SocketAddr =
        format!("{}:{}", host, port)
            .parse()
            .map_err(|e: std::net::AddrParseError| {
                AppCommandError::new(AppErrorCode::InvalidInput, ERR_INVALID_ADDRESS)
                    .with_detail(e.to_string())
            })?;

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(classify_bind_error)?;

    // Defend against socket-handle inheritance by later-spawned children
    // (terminals, ACP CLIs, git, etc.). Failure is logged but non-fatal:
    // the serve task still works; we just lose this defense-in-depth on
    // this start. See issue #126.
    if let Err(e) = socket_inherit::mark_listener_non_inheritable(&listener) {
        tracing::warn!("[WEB][WARN] failed to mark listener non-inheritable: {}", e);
    }

    // Persist only after a successful bind AND a successful strict-
    // mode check, so a misconfiguration doesn't overwrite saved state
    // and lock the desktop into a permanent "Web service won't start"
    // loop.
    persist_web_service_config(&app_state.db.conn, &token, port).await?;

    // Reset before any handler subscribes, so a leftover signal from the
    // previous cycle cannot make a new handler exit immediately.
    ws.shutdown_signal.reset();
    let shutdown_signal = ws.shutdown_signal.clone();

    // Sweep abandoned upload staging files from any previous run. Safe to
    // call before binding the listener; only touches `<uploads_root>/.tmp/`.
    handlers::files::purge_upload_staging().await;

    let router = router::build_router(
        app_state.clone(),
        token.clone(),
        static_assets::StaticAssetSource::directory(static_dir),
        shutdown_signal.clone(),
    );

    let local_addr = listener.local_addr().ok();
    let actual_port = local_addr.map(|a| a.port()).unwrap_or(port);
    // Advertise the IP the socket is actually bound to, not the raw config.
    let advertised_host = advertise_host(local_addr, &host);
    tracing::info!("[WEB] Starting web server on {}", addr);

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        let serve = axum::serve(listener, router).with_graceful_shutdown(async move {
            let _ = shutdown_rx.await;
        });
        if let Err(e) = serve.await {
            tracing::error!("[WEB] Server error: {}", e);
        }
    });

    *ws.handle.lock().unwrap() = Some(handle);
    *ws.shutdown_tx.lock().unwrap() = Some(shutdown_tx);
    ws.port.store(actual_port, Ordering::Relaxed);
    *ws.token.lock().unwrap() = token.clone();
    *ws.host.lock().unwrap() = advertised_host.clone();
    // running already true from compare_exchange; disarm guard so it doesn't flip back.
    guard.disarm();

    let addresses = addresses_for_bind(&advertised_host, actual_port);
    Ok(WebServerInfo {
        port: actual_port,
        token,
        addresses,
    })
}

pub(crate) async fn do_stop_web_server(state: &WebServerState) {
    let handle_opt = state.handle.lock().unwrap().take();
    let shutdown_tx = state.shutdown_tx.lock().unwrap().take();

    // Trigger first: sticky flag means any handshake completing during
    // the stop window also exits, not just currently-waiting handlers.
    // Without this, hyper's graceful drain would wait for live WS
    // connections and we'd always fall through to the abort branch.
    state.shutdown_signal.trigger();

    // Signal graceful shutdown so axum stops accepting new connections
    // and drops the listening socket once the serve future resolves.
    if let Some(tx) = shutdown_tx {
        let _ = tx.send(());
    }

    // Await the serve task so the OS socket is guaranteed released before we return.
    // A live WebSocket/keep-alive connection can block graceful drain; after a
    // short grace period, force-abort and await the cancellation to complete.
    if let Some(mut handle) = handle_opt {
        if tokio::time::timeout(std::time::Duration::from_secs(2), &mut handle)
            .await
            .is_err()
        {
            handle.abort();
            let _ = handle.await;
        }
    }

    // Only release the running flag after the listener is guaranteed dropped,
    // so a concurrent start() cannot race into a bind() while the old socket lingers.
    state.port.store(0, Ordering::Relaxed);
    *state.token.lock().unwrap() = String::new();
    *state.host.lock().unwrap() = "0.0.0.0".to_string();
    state.running.store(false, Ordering::Release);
    tracing::info!("[WEB] Web server stopped");
}

pub(crate) fn do_get_web_server_status(state: &WebServerState) -> Option<WebServerInfo> {
    if !state.running.load(Ordering::Relaxed) {
        return None;
    }
    let port = state.port.load(Ordering::Relaxed);
    let host = state.host.lock().unwrap().clone();
    let token = state.token.lock().unwrap().clone();
    let addresses = addresses_for_bind(&host, port);
    Some(WebServerInfo {
        port,
        token,
        addresses,
    })
}

/// Probe whether the configured port (or `override_port`) is currently
/// being LISTENed on by some process. Used by the settings page to
/// surface "stopped here, but the port is held by an orphan / another
/// process" — the scenario in issue #126.
pub(crate) async fn do_probe_web_service_port(
    conn: &DatabaseConnection,
    override_port: Option<u16>,
) -> Result<WebServicePortProbe, AppCommandError> {
    let port = resolve_web_service_port(conn, override_port).await?;
    let state = port_probe::probe_port(port).await;
    Ok(WebServicePortProbe { port, state })
}

// ── Tauri commands (thin wrappers) ──

#[cfg(feature = "tauri-runtime")]
pub(crate) async fn do_start_web_server_tauri(
    app: tauri::AppHandle,
    state: &WebServerState,
    port: Option<u16>,
    host: Option<String>,
    token: Option<String>,
) -> Result<WebServerInfo, AppCommandError> {
    // In Tauri mode, we still need to start via the legacy path because
    // the full AppState isn't easily available from tauri::State here.
    // The embedded web server uses Tauri's resource directory for static files.
    use tauri::Manager;

    let ws = state;

    // Atomically claim the running flag; concurrent starts see AlreadyExists.
    ws.running
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| AppCommandError::new(AppErrorCode::AlreadyExists, ERR_ALREADY_RUNNING))?;
    let mut guard = RunningGuard {
        running: &ws.running,
        armed: true,
    };

    let db = app.state::<crate::db::AppDatabase>();
    let port_val = resolve_web_service_port(&db.conn, port).await?;
    let host_val = host.unwrap_or_else(|| "0.0.0.0".to_string());
    let token = resolve_web_service_token(&db.conn, token).await?;

    // Same strict-mode validation as `do_start_web_server_with_state`:
    // run before any I/O so a misconfiguration cleanly fails the toggle
    // instead of taking the desktop down.
    handlers::files::log_upload_quota_config_at_startup();
    if let Err(err) = handlers::files::validate_upload_quota_config() {
        return Err(AppCommandError::new(
            AppErrorCode::InvalidInput,
            "Upload quota configuration is invalid",
        )
        .with_detail(err.to_string()));
    }

    let addr: SocketAddr =
        format!("{}:{}", host_val, port_val)
            .parse()
            .map_err(|e: std::net::AddrParseError| {
                AppCommandError::new(AppErrorCode::InvalidInput, ERR_INVALID_ADDRESS)
                    .with_detail(e.to_string())
            })?;

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(classify_bind_error)?;

    // See do_start_web_server_with_state for rationale.
    if let Err(e) = socket_inherit::mark_listener_non_inheritable(&listener) {
        tracing::warn!("[WEB][WARN] failed to mark listener non-inheritable: {}", e);
    }

    // Persist only after a successful bind AND strict-mode validation,
    // so a misconfiguration doesn't lock the desktop into a permanent
    // "Web service won't start" loop.
    persist_web_service_config(&db.conn, &token, port_val).await?;

    // Build AppState for the router
    let app_state = Arc::new(AppState {
        db: crate::db::AppDatabase {
            conn: app.state::<crate::db::AppDatabase>().conn.clone(),
        },
        agent_catalog: app
            .state::<crate::acp::version_center::CatalogStore>()
            .inner()
            .clone(),
        capability_policy: app
            .state::<crate::acp::capability_policy::CapabilityPolicyStore>()
            .inner()
            .clone(),
        capability_policy_refresh: app
            .state::<Arc<crate::acp::capability_policy::CapabilityPolicyRefreshRuntime>>()
            .inner()
            .clone(),
        plugin_registry: app
            .state::<crate::plugin_runtime::registry::PluginRegistry>()
            .inner()
            .clone(),
        plugin_supervisor: app
            .state::<Arc<crate::plugin_runtime::supervisor::PluginRuntimeSupervisor>>()
            .inner()
            .clone(),
        plugin_router: app
            .state::<crate::plugin_runtime::router::PluginRouter>()
            .inner()
            .clone(),
        connection_manager: (*app.state::<crate::acp::manager::ConnectionManager>()).clone_ref(),
        terminal_manager: (*app.state::<crate::terminal::manager::TerminalManager>()).clone_ref(),
        event_broadcaster: app
            .state::<Arc<crate::web::event_bridge::WebEventBroadcaster>>()
            .inner()
            .clone(),
        // Reuse the same bus the Tauri webview & subscribers read from.
        acp_event_bus: app
            .state::<Arc<crate::acp::InternalEventBus>>()
            .inner()
            .clone(),
        emitter: crate::web::event_bridge::EventEmitter::Tauri(app.clone()),
        // Resolve through the effective data dir so a custom
        // `IYW_CLAW_DATA_DIR` reaches the credential helper and any HTTP
        // handler that reads `state.data_dir`.
        data_dir: crate::paths::resolve_effective_data_dir(
            &app.path().app_data_dir().unwrap_or_default(),
        ),
        web_server_state: WebServerState::new(), // placeholder; not used by handlers
        chat_channel_manager: app
            .state::<crate::chat_channel::manager::ChatChannelManager>()
            .clone_ref(),
        workspace_transfer: app
            .try_state::<Arc<crate::workspace_transfer::WorkspaceTransferManager>>()
            .map(|state| state.inner().clone())
            .unwrap_or_else(|| {
                Arc::new(crate::workspace_transfer::WorkspaceTransferManager::new_from_env())
            }),
        // Reuse the live broker and token registry from Tauri-managed state.
        delegation_broker: app
            .state::<Arc<crate::acp::delegation::broker::DelegationBroker>>()
            .inner()
            .clone(),
        delegation_tokens: app
            .state::<Arc<crate::acp::delegation::listener::TokenRegistry>>()
            .inner()
            .clone(),
        // Reuse the same live-feedback config handle the desktop MCP injection
        // reads, so HTTP-side feedback settings target the identical flag.
        feedback_config: app
            .state::<crate::acp::feedback::FeedbackRuntimeConfig>()
            .inner()
            .clone(),
        // Reuse the same ask-user-question config handle the desktop MCP
        // injection reads, so HTTP-side question settings target the same flag.
        question_config: app
            .state::<crate::acp::question::QuestionRuntimeConfig>()
            .inner()
            .clone(),
        // Reuse the same get-session-info config handle the desktop MCP injection
        // reads, so HTTP-side session-info settings target the same flag.
        session_info_config: app
            .state::<crate::acp::session_info::SessionInfoRuntimeConfig>()
            .inner()
            .clone(),
        user_memory: app
            .state::<Arc<crate::user_memory::UserMemoryService>>()
            .inner()
            .clone(),
        system_op_lock: crate::app_state::default_system_op_lock(),
        // Reuse the same handle the desktop `app_update` commands write to so
        // HTTP and webview readers see the identical update snapshot.
        update_state: app
            .state::<crate::update::AppUpdateStateHandle>()
            .inner()
            .clone(),
    });

    // See do_start_web_server_with_state for rationale on the reset.
    ws.shutdown_signal.reset();
    let shutdown_signal = ws.shutdown_signal.clone();

    // Sweep abandoned upload staging files. See the matching call in
    // `do_start_web_server_with_state` for rationale. Quota log/validate
    // already ran earlier in this function before the bind.
    handlers::files::purge_upload_staging().await;

    let router = router::build_router(
        app_state,
        token.clone(),
        static_assets::StaticAssetSource::tauri(app.clone()),
        shutdown_signal.clone(),
    );

    let local_addr = listener.local_addr().ok();
    let actual_port = local_addr.map(|a| a.port()).unwrap_or(port_val);
    // Advertise the IP the socket is actually bound to, not the raw config.
    let advertised_host = advertise_host(local_addr, &host_val);
    tracing::info!("[WEB] Starting web server on {}", addr);

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        let serve = axum::serve(listener, router).with_graceful_shutdown(async move {
            let _ = shutdown_rx.await;
        });
        if let Err(e) = serve.await {
            tracing::error!("[WEB] Server error: {}", e);
        }
    });

    *ws.handle.lock().unwrap() = Some(handle);
    *ws.shutdown_tx.lock().unwrap() = Some(shutdown_tx);
    ws.port.store(actual_port, Ordering::Relaxed);
    *ws.token.lock().unwrap() = token.clone();
    *ws.host.lock().unwrap() = advertised_host.clone();
    // running already true from compare_exchange; disarm guard so it doesn't flip back.
    guard.disarm();

    let addresses = addresses_for_bind(&advertised_host, actual_port);
    Ok(WebServerInfo {
        port: actual_port,
        token,
        addresses,
    })
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn start_web_server(
    app: tauri::AppHandle,
    state: tauri::State<'_, WebServerState>,
    port: Option<u16>,
    host: Option<String>,
    token: Option<String>,
) -> Result<WebServerInfo, AppCommandError> {
    do_start_web_server_tauri(app, &state, port, host, token).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn stop_web_server(
    state: tauri::State<'_, WebServerState>,
) -> Result<(), AppCommandError> {
    do_stop_web_server(&state).await;
    Ok(())
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn get_web_server_status(
    state: tauri::State<'_, WebServerState>,
) -> Result<Option<WebServerInfo>, AppCommandError> {
    Ok(do_get_web_server_status(&state))
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn get_web_service_config(
    db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<WebServiceConfig, AppCommandError> {
    load_web_service_config(&db.conn).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn update_web_service_config(
    db: tauri::State<'_, crate::db::AppDatabase>,
    config: WebServiceConfig,
) -> Result<WebServiceConfig, AppCommandError> {
    update_web_service_config_core(&db.conn, config).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn probe_web_service_port(
    db: tauri::State<'_, crate::db::AppDatabase>,
    port: Option<u16>,
) -> Result<WebServicePortProbe, AppCommandError> {
    do_probe_web_service_port(&db.conn, port).await
}
