use std::collections::BTreeMap;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use rmcp::ErrorData;
use sea_orm::DatabaseConnection;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::acp::account_credentials::AccountAccessToken;

mod catalog;
mod connection;
mod directory;
mod request;

use catalog::RemoteRoute;
use connection::RemoteConnection;

const FAILURE_COOLDOWN: Duration = Duration::from_secs(15);
pub(super) const REMOTE_PREFIX: &str = "iyw.remote.";
pub(super) const AGENT_INSTRUCTIONS: &str = "The same search/read/invoke trio includes signed-in remote business capabilities. Use search source=local for host work, source=remote for enterprise/prospecting/customs/ecommerce/brand/copyright/product/web/speech/retrieval work, or all when unsure. Prefer a matching direct tool. Read group workflows and complete item schemas/usage; invoke an item's capability_id, never the group. A member fully described in a group read needs no additional read. The host carries remote IDs and versions. TOOL_CHANGED with not_started requires rereading the old capability_id and using the current ID returned; remote_catalog_expired permits fresh discovery. Remote catalog unavailable is not evidence of absence. Preserve authorization and original task identity; never replay an uncertain business operation.";

pub(super) struct RemoteGateway {
    db: DatabaseConnection,
    account: Mutex<Option<Arc<RemoteAccount>>>,
    shutdown: CancellationToken,
    warmup: StdMutex<Option<tokio::task::JoinHandle<()>>>,
}

struct RemoteAccount {
    fingerprint: [u8; 32],
    cancellation: CancellationToken,
    connection: Mutex<ConnectionState>,
    routes: StdMutex<BTreeMap<String, RemoteRoute>>,
}

#[derive(Default)]
struct ConnectionState {
    client: Option<Arc<RemoteConnection>>,
    retry_after: Option<Instant>,
}

#[derive(Clone, Copy)]
pub(super) struct RemoteContext<'a> {
    pub request_cancel: &'a CancellationToken,
    pub authority_cancel: &'a CancellationToken,
}

impl RemoteGateway {
    pub(super) fn new(db: DatabaseConnection) -> Arc<Self> {
        Arc::new(Self {
            db,
            account: Mutex::new(None),
            shutdown: CancellationToken::new(),
            warmup: StdMutex::new(None),
        })
    }

    pub(super) fn prewarm(self: &Arc<Self>) {
        let mut warmup = self
            .warmup
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if warmup.as_ref().is_some_and(|task| !task.is_finished()) || self.shutdown.is_cancelled() {
            return;
        }
        let gateway = self.clone();
        *warmup = Some(tokio::spawn(async move {
            if let Err(error) = gateway.ready().await {
                log_failure("prewarm", &error);
            }
        }));
    }

    async fn ready_for(
        &self,
        context: RemoteContext<'_>,
    ) -> Result<(Arc<RemoteAccount>, Arc<RemoteConnection>), ErrorData> {
        tokio::select! {
            biased;
            _ = context.request_cancel.cancelled() => Err(failure("remote_cancelled", "Request cancelled", false)),
            _ = context.authority_cancel.cancelled() => Err(failure("remote_cancelled", "Session revoked", false)),
            result = self.ready() => result,
        }
    }

    async fn ready(&self) -> Result<(Arc<RemoteAccount>, Arc<RemoteConnection>), ErrorData> {
        let (account, token) = self.current_account().await?;
        let mut state = account.connection.lock().await;
        if account.cancellation.is_cancelled() {
            return Err(failure(
                "remote_account_changed",
                "Remote account changed",
                false,
            ));
        }
        if let Some(client) = &state.client {
            if !client.is_closed() {
                return Ok((account.clone(), client.clone()));
            }
        }
        if state
            .retry_after
            .is_some_and(|until| until > Instant::now())
        {
            return Err(failure(
                "remote_cooling_down",
                "Remote connection temporarily unavailable; retry later",
                false,
            ));
        }
        let client = state.refresh(token.expose(), &account.cancellation).await?;
        Ok((account.clone(), client))
    }

    async fn current_account(&self) -> Result<(Arc<RemoteAccount>, AccountAccessToken), ErrorData> {
        let mut current = self.account.lock().await;
        let token = crate::commands::iyw_account::iyw_account_access_token_core(&self.db)
            .await
            .map_err(|_| {
                failure(
                    "remote_credentials_unavailable",
                    "Cannot load current account",
                    false,
                )
            })?;
        let fingerprint = token
            .as_ref()
            .map(|token| Sha256::digest(token.expose().as_bytes()).into());
        bind_account(&mut current, fingerprint, &self.shutdown);
        let Some(token) = token else {
            return Err(failure(
                "remote_sign_in_required",
                "Sign in to use remote business capabilities",
                false,
            ));
        };
        if self.shutdown.is_cancelled() {
            return Err(failure(
                "remote_shutting_down",
                "Remote gateway is shutting down",
                false,
            ));
        }
        let account = current
            .as_ref()
            .cloned()
            .ok_or_else(|| failure("remote_account_changed", "Account changed", false))?;
        Ok((account, token))
    }

    async fn ensure_current(&self, account: &RemoteAccount) -> Result<(), ErrorData> {
        let (current, _) = self.current_account().await?;
        if current.fingerprint != account.fingerprint || account.cancellation.is_cancelled() {
            return Err(failure(
                "remote_account_changed",
                "Account changed; search the current directory again",
                false,
            ));
        }
        Ok(())
    }

    pub(super) async fn shutdown(&self) -> bool {
        self.shutdown.cancel();
        let warmup = self
            .warmup
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        if let Some(task) = warmup {
            task.abort();
            let _ = task.await;
        }
        let account = self.account.lock().await.as_ref().cloned();
        if let Some(account) = account {
            let client = account.connection.lock().await.client.as_ref().cloned();
            if let Some(client) = client {
                return client.close().await;
            }
        }
        true
    }
}

impl ConnectionState {
    async fn refresh(
        &mut self,
        token: &str,
        cancellation: &CancellationToken,
    ) -> Result<Arc<RemoteConnection>, ErrorData> {
        self.client = None;
        let started = Instant::now();
        match RemoteConnection::open(token, cancellation).await {
            Ok(client) => {
                tracing::info!(
                    elapsed_ms = started.elapsed().as_millis(),
                    "[remote-mcp] connection initialized"
                );
                self.retry_after = None;
                self.client = Some(client.clone());
                Ok(client)
            }
            Err(error) => {
                self.retry_after = Some(Instant::now() + FAILURE_COOLDOWN);
                log_failure("initialize", &error);
                Err(error)
            }
        }
    }
}

fn bind_account(
    current: &mut Option<Arc<RemoteAccount>>,
    fingerprint: Option<[u8; 32]>,
    shutdown: &CancellationToken,
) {
    if current.as_ref().map(|account| account.fingerprint) == fingerprint {
        return;
    }
    if let Some(previous) = current.take() {
        previous.cancellation.cancel();
        tracing::info!("[remote-mcp] account changed; previous connection and directory revoked");
    }
    *current = fingerprint.map(|fingerprint| {
        Arc::new(RemoteAccount {
            fingerprint,
            cancellation: shutdown.child_token(),
            connection: Mutex::new(ConnectionState::default()),
            routes: StdMutex::new(BTreeMap::new()),
        })
    });
}

impl Drop for RemoteGateway {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}

fn failure(code: &str, message: &str, execution: bool) -> ErrorData {
    ErrorData::internal_error(
        message.to_string(),
        Some(json!({
            "code": code,
            "execution_status": if execution { "unknown" } else { "not_started" },
            "retryable": false,
        })),
    )
}

fn log_failure(stage: &str, error: &ErrorData) {
    let code = error
        .data
        .as_ref()
        .and_then(|value| value.get("code"))
        .and_then(Value::as_str)
        .unwrap_or("remote_error");
    if stage == "prewarm" && code == "remote_sign_in_required" {
        tracing::debug!("[remote-mcp] prewarm skipped without a signed-in account");
        return;
    }
    tracing::warn!(
        stage,
        code,
        rpc_code = error.code.0,
        "[remote-mcp] request failed"
    );
}
