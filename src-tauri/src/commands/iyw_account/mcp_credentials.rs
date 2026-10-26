use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, Instant};

use sea_orm::DatabaseConnection;
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use crate::acp::account_credentials::AccountAccessToken;

use super::{session, types::StoredSession, AppCommandError, AppErrorCode};

const RENEW_THRESHOLD_MS: i64 = 30 * 60 * 1000;
const RENEW_COOLDOWN: Duration = Duration::from_secs(30);
const MAINTENANCE_TIMEOUT: Duration = Duration::from_secs(20);

struct Snapshot {
    session: Arc<StoredSession>,
    fingerprint: [u8; 32],
    revision: u64,
    cancellation: CancellationToken,
}

#[derive(Default)]
struct State {
    current: Option<Arc<Snapshot>>,
    revision: u64,
}

pub(super) struct PublicationGuard(bool);

impl PublicationGuard {
    pub(super) fn new() -> Self {
        Self(false)
    }

    pub(super) fn finish(mut self) {
        self.0 = true;
    }
}

impl Drop for PublicationGuard {
    fn drop(&mut self) {
        if !self.0 {
            crate::acp::model_gateway::update_credentials(None, None, false);
            invalidate();
        }
    }
}

fn state() -> &'static RwLock<State> {
    static STATE: OnceLock<RwLock<State>> = OnceLock::new();
    STATE.get_or_init(|| RwLock::new(State::default()))
}

fn unavailable() -> AppCommandError {
    AppCommandError::authentication_failed("账号凭证不可用，请刷新账号信息或重新登录")
}

pub(super) fn publish(session: &StoredSession) {
    let identity = (
        &session.session_id,
        &session.org_code,
        session
            .token
            .as_ref()
            .map(|token| token.access_token.trim()),
        session.workspace_pending,
    );
    let Ok(encoded) = serde_json::to_vec(&identity) else {
        invalidate();
        return;
    };
    let fingerprint = Sha256::digest(encoded).into();
    let mut state = state().write().unwrap_or_else(|error| error.into_inner());
    let unchanged = state.current.as_ref().filter(|snapshot| {
        snapshot.fingerprint == fingerprint && !snapshot.cancellation.is_cancelled()
    });
    let cancellation = unchanged.map(|snapshot| snapshot.cancellation.clone());
    if cancellation.is_none() {
        if let Some(previous) = state.current.take() {
            previous.cancellation.cancel();
        }
        state.revision = state.revision.wrapping_add(1);
        tracing::debug!(
            revision = state.revision,
            "[remote-mcp] account credentials published"
        );
    }
    state.current = Some(Arc::new(Snapshot {
        session: Arc::new(session.clone()),
        fingerprint,
        revision: state.revision,
        cancellation: cancellation.unwrap_or_default(),
    }));
}

pub(super) fn invalidate() {
    let mut state = state().write().unwrap_or_else(|error| error.into_inner());
    if let Some(previous) = state.current.take() {
        previous.cancellation.cancel();
        state.revision = state.revision.wrapping_add(1);
        tracing::warn!(
            revision = state.revision,
            "[remote-mcp] account credentials revoked"
        );
    }
}

fn current() -> Result<Arc<Snapshot>, AppCommandError> {
    if session::workspace_switching() {
        return Err(AppCommandError::permission_denied(
            "空间正在切换，请稍后重试",
        ));
    }
    state()
        .read()
        .map_err(|_| unavailable())?
        .current
        .clone()
        .ok_or_else(unavailable)
}

pub(crate) fn read(
) -> Result<(Option<AccountAccessToken>, [u8; 32], CancellationToken), AppCommandError> {
    credentials(current()?.as_ref())
}

fn credentials(
    snapshot: &Snapshot,
) -> Result<(Option<AccountAccessToken>, [u8; 32], CancellationToken), AppCommandError> {
    if snapshot.cancellation.is_cancelled() || snapshot.session.workspace_pending {
        return Err(unavailable());
    }
    if snapshot.session.token.is_some()
        && snapshot
            .session
            .expires_at_ms
            .is_none_or(|expiration| session::now_ms() >= expiration)
    {
        return Err(unavailable());
    }
    let token = snapshot
        .session
        .token
        .as_ref()
        .and_then(|token| AccountAccessToken::new(token.access_token.clone()));
    Ok((token, snapshot.fingerprint, snapshot.cancellation.clone()))
}

pub(crate) async fn maintain(conn: &DatabaseConnection) {
    let Ok(before) = current() else { return };
    if before.session.workspace_pending
        || before.session.token.is_none()
        || before.session.expires_at_ms.is_some_and(|expiration| {
            session::now_ms() < expiration.saturating_sub(RENEW_THRESHOLD_MS)
        })
    {
        return;
    }
    static MAINTENANCE: OnceLock<tokio::sync::Mutex<Option<(u64, Instant)>>> = OnceLock::new();
    let Ok(mut retry) = MAINTENANCE.get_or_init(Default::default).try_lock() else {
        return;
    };
    if before.cancellation.is_cancelled()
        || retry
            .is_some_and(|(revision, until)| revision == before.revision && until > Instant::now())
    {
        return;
    }
    *retry = Some((before.revision, Instant::now() + RENEW_COOLDOWN));
    match tokio::time::timeout(
        MAINTENANCE_TIMEOUT,
        session::renew_session(conn, &before.session),
    )
    .await
    {
        Ok(Ok(_)) => {}
        Ok(Err(error)) => {
            tracing::warn!(stage = "mcp_renewal", code = ?error.code,
                "[remote-mcp] account maintenance failed");
            if error.code == AppErrorCode::AuthenticationFailed {
                invalidate_if_current(&before);
            }
        }
        Err(_) => tracing::warn!(
            stage = "mcp_renewal",
            "[remote-mcp] account maintenance timed out"
        ),
    }
}

fn invalidate_if_current(before: &Snapshot) {
    let mut state = state().write().unwrap_or_else(|error| error.into_inner());
    if state.current.as_ref().is_some_and(|current| {
        current.revision == before.revision
            && current.session.expires_at_ms == before.session.expires_at_ms
    }) {
        if let Some(previous) = state.current.take() {
            previous.cancellation.cancel();
        }
        state.revision = state.revision.wrapping_add(1);
    }
}
