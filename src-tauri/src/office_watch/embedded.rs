//! 文档解析直接链接进主程序；每个预览仅持有可取消任务和回环监听器。
use std::future::IntoFuture;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::sync::{watch, Semaphore};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::WatchError;

#[path = "embedded_http.rs"]
mod http;
#[path = "embedded_import.rs"]
mod import;
#[path = "embedded_snapshot.rs"]
pub(crate) mod snapshot;

static IMPORT_SLOTS: Semaphore = Semaphore::const_new(2);
const POLL_DELAY: Duration = Duration::from_millis(350);

#[derive(Clone, Serialize)]
pub(super) struct Snapshot {
    pub generation: String,
    pub error: Option<String>,
}

pub(super) struct State {
    pub root: Arc<TempDir>,
    pub cap: String,
    pub grid: bool,
    pub versions: watch::Sender<Snapshot>,
    pub cancel: CancellationToken,
}

pub(super) struct Worker {
    cancel: CancellationToken,
    task: JoinHandle<()>,
}

struct Document {
    file: PathBuf,
    cancel: CancellationToken,
    stamp: Option<(u64, Option<std::time::SystemTime>)>,
}

impl Worker {
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        Ok(self.task.is_finished().then(ExitStatus::default))
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

pub(super) async fn spawn(file: &Path, port: u16, cap: &str) -> Result<Worker, WatchError> {
    let cancel = CancellationToken::new();
    let cancelled_start = cancel.clone().drop_guard();
    let root = Arc::new(tempfile::tempdir()?);
    let stamp = super::file_state(file);
    let generation = import::convert(file, root.clone(), cancel.clone()).await?;
    let listener = TcpListener::bind(("127.0.0.1", port)).await?;
    let (versions, _) = watch::channel(Snapshot {
        generation,
        error: None,
    });
    let state = Arc::new(State {
        root,
        cap: cap.to_owned(),
        grid: file
            .extension()
            .is_some_and(|value| value.eq_ignore_ascii_case("xlsx")),
        versions,
        cancel: cancel.clone(),
    });
    let document = Document {
        file: file.to_owned(),
        cancel: cancel.clone(),
        stamp,
    };
    let task = tokio::spawn(run(listener, state, document));
    cancelled_start.disarm();
    Ok(Worker { cancel, task })
}

async fn run(listener: TcpListener, state: Arc<State>, document: Document) {
    let app = http::router(state.clone());
    let server_cancel = document.cancel.clone();
    let cancel = document.cancel.clone();
    let server = axum::serve(listener, app)
        .with_graceful_shutdown(server_cancel.cancelled_owned())
        .into_future();
    let serving = async {
        tokio::pin!(server);
        tokio::select! {
            result = &mut server => {
                if let Err(error) = result { tracing::warn!(%error, "[office-preview] HTTP listener stopped"); }
            }
            _ = cancel.cancelled() => {
                let _ = tokio::time::timeout(Duration::from_secs(2), &mut server).await;
            }
        }
        cancel.cancel();
    };
    tokio::join!(serving, monitor(document, state));
}

async fn monitor(document: Document, state: Arc<State>) {
    let Document {
        file,
        cancel,
        stamp: mut applied,
    } = document;
    let mut previous = applied;
    let mut failed = false;
    let mut generations = vec![state.versions.borrow().generation.clone()];
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = tokio::time::sleep(POLL_DELAY) => {}
        }
        let current = super::file_state(&file);
        if current != previous {
            previous = current;
            failed = false;
            continue;
        }
        if current == applied || failed {
            continue;
        }
        match import::convert(&file, state.root.clone(), cancel.clone()).await {
            Ok(generation) => {
                applied = current;
                publish(&state, generation, &mut generations).await;
            }
            Err(_) if cancel.is_cancelled() => break,
            Err(error) => {
                failed = true;
                tracing::warn!(%error, "[office-preview] document refresh failed");
                let generation = state.versions.borrow().generation.clone();
                state.versions.send_replace(Snapshot {
                    generation,
                    error: Some(error.to_string()),
                });
            }
        }
    }
}

async fn publish(state: &State, generation: String, generations: &mut Vec<String>) {
    state.versions.send_replace(Snapshot {
        generation: generation.clone(),
        error: None,
    });
    generations.push(generation);
    if generations.len() > 2 {
        let stale = state.root.path().join(generations.remove(0));
        if let Err(error) = tokio::fs::remove_dir_all(stale).await {
            tracing::debug!(%error, "[office-preview] stale generation cleanup deferred");
        }
    }
}
