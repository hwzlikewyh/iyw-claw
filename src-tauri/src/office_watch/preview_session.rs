use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, LazyLock, Mutex,
};
use std::time::{Duration, Instant};

use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use super::{OfficeWatchStarted, WatchOrigin};
use crate::app_error::AppCommandError;

const SESSION_LIMIT: usize = 128;
const SESSION_TIMEOUT: Duration = Duration::from_secs(90);
static SESSIONS: LazyLock<Mutex<HashMap<String, Arc<Session>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static START_SLOTS: LazyLock<tokio::sync::Semaphore> =
    LazyLock::new(|| tokio::sync::Semaphore::new(2));

struct Session {
    cancelled: CancellationToken,
    acquired: AtomicBool,
    touched: Mutex<Instant>,
    target: Mutex<Option<(String, String, WatchOrigin)>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenParams {
    pub id: String,
    pub root_path: String,
    pub path: String,
}

#[derive(Deserialize)]
pub struct IdParams {
    pub id: String,
}

fn session(id: &str) -> Result<Arc<Session>, AppCommandError> {
    if uuid::Uuid::parse_str(id).is_err() {
        return Err(AppCommandError::invalid_input("Invalid preview session"));
    }
    let mut sessions = SESSIONS.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(session) = sessions.get(id) {
        return Ok(session.clone());
    }
    if sessions.len() >= SESSION_LIMIT {
        return Err(AppCommandError::invalid_input("Too many preview sessions"));
    }
    let entry = Arc::new(Session {
        cancelled: CancellationToken::new(),
        acquired: AtomicBool::new(false),
        touched: Mutex::new(Instant::now()),
        target: Mutex::new(None),
    });
    sessions.insert(id.to_string(), entry.clone());
    expire(id.to_string(), entry.clone());
    Ok(entry)
}

pub async fn open(
    params: OpenParams,
    origin: WatchOrigin,
) -> Result<OfficeWatchStarted, AppCommandError> {
    let entry = session(&params.id)?;
    {
        let mut target = entry
            .target
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if target.is_some() {
            return Err(AppCommandError::invalid_input(
                "Preview session already used",
            ));
        }
        *target = Some((params.root_path.clone(), params.path.clone(), origin));
    }
    let started = tokio::select! {
        _ = entry.cancelled.cancelled() => return Err(AppCommandError::invalid_input("Preview closed")),
        result = start(params, origin) => result?,
    };
    entry.acquired.store(true, Ordering::SeqCst);
    if entry.cancelled.is_cancelled() {
        release(&entry).await;
        return Err(AppCommandError::invalid_input("Preview closed"));
    }
    tracing::debug!("[office-preview] preview session ready");
    Ok(started)
}

async fn start(
    params: OpenParams,
    origin: WatchOrigin,
) -> Result<OfficeWatchStarted, AppCommandError> {
    let _permit = START_SLOTS
        .acquire()
        .await
        .map_err(|_| AppCommandError::invalid_input("Preview startup unavailable"))?;
    super::start_office_watch_core(params.root_path, params.path, origin)
        .await
        .map_err(Into::into)
}

pub async fn close(id: &str) -> Result<(), AppCommandError> {
    let entry = session(id)?;
    entry.cancelled.cancel();
    release(&entry).await;
    Ok(())
}

async fn release(entry: &Session) {
    if !entry.acquired.swap(false, Ordering::SeqCst) {
        return;
    }
    let target = entry
        .target
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    if let Some((root, path, origin)) = target {
        if let Err(error) = super::stop_office_watch_core(root, path, origin).await {
            tracing::warn!(%error, "[office-preview] session release failed");
        }
    }
}

pub fn renew(id: &str) -> Result<(), AppCommandError> {
    let sessions = SESSIONS.lock().unwrap_or_else(|error| error.into_inner());
    let entry = sessions
        .get(id)
        .filter(|entry| !entry.cancelled.is_cancelled())
        .ok_or_else(|| AppCommandError::not_found("Preview session expired"))?;
    *entry
        .touched
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Instant::now();
    Ok(())
}

fn expire(id: String, entry: Arc<Session>) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(SESSION_TIMEOUT).await;
            let elapsed = entry
                .touched
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .elapsed();
            if entry.cancelled.is_cancelled() || elapsed >= SESSION_TIMEOUT {
                entry.cancelled.cancel();
                release(&entry).await;
                SESSIONS
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .remove(&id);
                break;
            }
        }
    });
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn open_office_preview(
    id: String,
    root_path: String,
    path: String,
) -> Result<OfficeWatchStarted, AppCommandError> {
    open(
        OpenParams {
            id,
            root_path,
            path,
        },
        WatchOrigin::Desktop,
    )
    .await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn close_office_preview(id: String) -> Result<(), AppCommandError> {
    close(&id).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn renew_office_preview(id: String) -> Result<(), AppCommandError> {
    renew(&id)
}

pub async fn open_handler(
    axum::Json(params): axum::Json<OpenParams>,
) -> Result<axum::Json<OfficeWatchStarted>, AppCommandError> {
    open(params, WatchOrigin::Web).await.map(axum::Json)
}

pub async fn close_handler(
    axum::Json(params): axum::Json<IdParams>,
) -> Result<axum::Json<()>, AppCommandError> {
    close(&params.id).await.map(axum::Json)
}

pub async fn renew_handler(
    axum::Json(params): axum::Json<IdParams>,
) -> Result<axum::Json<()>, AppCommandError> {
    renew(&params.id).map(axum::Json)
}
