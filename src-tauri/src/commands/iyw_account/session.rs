use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use sea_orm::DatabaseConnection;

use super::{app_metadata_service, token_file, types::StoredSession, AppCommandError};

const SESSION_KEY: &str = "iyw_account_session";
const RENEW_THRESHOLD_MS: i64 = 30 * 60 * 1000;
const MILLISECONDS_PER_SECOND: i64 = 1000;
static WORKSPACE_SWITCHING: AtomicBool = AtomicBool::new(false);

pub(crate) struct WorkspaceSwitchGuard;

impl WorkspaceSwitchGuard {
    pub fn begin() -> Result<Self, AppCommandError> {
        WORKSPACE_SWITCHING
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| AppCommandError::permission_denied("空间正在切换，请稍后重试"))?;
        Ok(Self)
    }
}

impl Drop for WorkspaceSwitchGuard {
    fn drop(&mut self) {
        WORKSPACE_SWITCHING.store(false, Ordering::Release);
    }
}

pub(crate) fn workspace_switching() -> bool {
    WORKSPACE_SWITCHING.load(Ordering::Acquire)
}

pub(super) fn operation_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn persistence_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

pub(super) fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub(super) fn expires_at(seconds: &str) -> Result<i64, AppCommandError> {
    seconds
        .parse::<i64>()
        .ok()
        .filter(|seconds| *seconds > 0)
        .and_then(|seconds| seconds.checked_mul(MILLISECONDS_PER_SECOND))
        .and_then(|milliseconds| now_ms().checked_add(milliseconds))
        .ok_or_else(|| AppCommandError::network("Account expiration is invalid"))
}

pub(super) async fn load_session(
    conn: &DatabaseConnection,
) -> Result<StoredSession, AppCommandError> {
    let raw = app_metadata_service::get_value(conn, SESSION_KEY)
        .await
        .map_err(AppCommandError::from)?;
    match raw {
        Some(raw) => serde_json::from_str(&raw).map_err(|_| {
            AppCommandError::configuration_invalid("Unable to read stored account session")
        }),
        None => Ok(StoredSession::default()),
    }
}

async fn persist(
    conn: &DatabaseConnection,
    session: &StoredSession,
) -> Result<(), AppCommandError> {
    let serialized = serde_json::to_string(session)
        .map_err(|_| AppCommandError::configuration_invalid("Unable to save account session"))?;
    token_file::sync(session.token.as_ref())?;
    app_metadata_service::upsert_value(conn, SESSION_KEY, &serialized)
        .await
        .map_err(AppCommandError::from)
}

pub(super) async fn save_session(
    conn: &DatabaseConnection,
    session: &StoredSession,
) -> Result<(), AppCommandError> {
    let _guard = persistence_lock().lock().await;
    persist(conn, session).await
}

pub(super) fn same_session(before: &StoredSession, after: &StoredSession) -> bool {
    before.session_id == after.session_id
        && before.org_code == after.org_code
        && before.token.as_ref().map(|token| &token.access_token)
            == after.token.as_ref().map(|token| &token.access_token)
}

pub(super) async fn save_if_current(
    conn: &DatabaseConnection,
    before: &StoredSession,
    next: &StoredSession,
) -> Result<bool, AppCommandError> {
    let _guard = persistence_lock().lock().await;
    let current = load_session(conn).await?;
    if !same_session(before, &current) {
        return Ok(false);
    }
    let mut next = next.clone();
    if current.expires_at_ms != before.expires_at_ms {
        next.expires_at_ms = current.expires_at_ms;
        if let (Some(token), Some(current_token)) = (next.token.as_mut(), current.token.as_ref()) {
            token.expiration = current_token.expiration.clone();
        }
    }
    persist(conn, &next).await?;
    Ok(true)
}

pub(super) async fn load_synced_session(
    conn: &DatabaseConnection,
) -> Result<StoredSession, AppCommandError> {
    let _guard = persistence_lock().lock().await;
    let session = load_session(conn).await?;
    token_file::sync(session.token.as_ref())?;
    Ok(session)
}

pub(super) async fn renew_session(
    conn: &DatabaseConnection,
    before: &StoredSession,
) -> Result<StoredSession, AppCommandError> {
    static RENEW: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    let _guard = RENEW
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await;
    let current = load_session(conn).await?;
    if !same_session(before, &current) || current.expires_at_ms != before.expires_at_ms {
        return Ok(current);
    }
    let token = current
        .token
        .as_ref()
        .ok_or_else(|| AppCommandError::authentication_failed("请重新登录"))?;
    let response = super::wire::http_client()?
        .get("https://i.iyw.cn/api/sso/api/sso/web/renewToken")
        .query(&[("refreshToken", &token.refresh_token)])
        .send()
        .await
        .map_err(|error| {
            AppCommandError::network("Account renewal is temporarily unavailable")
                .with_detail(error.without_url().to_string())
        })?;
    let body = super::wire::decode(response, "renewToken").await?;
    if !matches!(super::types::code(&body), Some(1 | 413)) {
        return Err(super::account_error(
            super::types::field(&body, "message"),
            "登录已失效，请重新登录",
        ));
    }
    let expiration = body
        .get("data")
        .and_then(super::types::scalar)
        .ok_or_else(|| AppCommandError::network("Account renewal duration is missing"))?;
    let mut next = current.clone();
    next.expires_at_ms = Some(expires_at(&expiration)?);
    if let Some(token) = next.token.as_mut() {
        token.expiration = expiration;
    }
    if !save_if_current(conn, &current, &next).await? {
        return load_session(conn).await;
    }
    tracing::info!(stage = "renewal", "[iyw-account] account session renewed");
    Ok(next)
}

pub(super) async fn fresh_session(
    conn: &DatabaseConnection,
) -> Result<StoredSession, AppCommandError> {
    let session = load_synced_session(conn).await?;
    if session.token.is_none() {
        return Ok(session);
    }
    if session
        .expires_at_ms
        .is_none_or(|expiration| now_ms() >= expiration - RENEW_THRESHOLD_MS)
    {
        match renew_session(conn, &session).await {
            Ok(next) => return Ok(next),
            Err(error) if error.code == super::AppErrorCode::AuthenticationFailed => {
                return Err(error)
            }
            Err(error) => tracing::warn!(stage = "renewal", code = ?error.code,
                "[iyw-account] renewal unavailable; retaining existing session"),
        }
    }
    Ok(session)
}

pub(crate) async fn iyw_account_memory_credentials_core(
    conn: &DatabaseConnection,
) -> Result<Option<(crate::acp::account_credentials::AccountAccessToken, String)>, AppCommandError>
{
    use sha2::{Digest, Sha256};
    super::balance_cache::ensure_can_consume(conn).await?;
    let session = fresh_session(conn).await?;
    let Some(token) = session.token else {
        return Ok(None);
    };
    let identity = if token.refresh_token.is_empty() {
        &token.access_token
    } else {
        &token.refresh_token
    };
    let scope = format!(
        "{:x}",
        Sha256::digest(
            format!(
                "{}:{}",
                identity,
                session.org_code.as_deref().unwrap_or_default()
            )
            .as_bytes()
        )
    );
    Ok(
        crate::acp::account_credentials::AccountAccessToken::new(token.access_token)
            .map(|token| (token, scope)),
    )
}

pub(crate) async fn iyw_account_product_context_core(
    conn: &DatabaseConnection,
) -> Result<(String, String), AppCommandError> {
    let session = fresh_session(conn).await?;
    if session.workspace_pending || session.business_credentials_pending {
        return Err(AppCommandError::permission_denied(
            "企业服务凭证尚未同步，请在空间菜单中重新同步",
        ));
    }
    let token = session.product_token.ok_or_else(|| {
        AppCommandError::permission_denied("企业服务凭证缺失，请在空间菜单中重新同步")
    })?;
    let org = session
        .org_code
        .ok_or_else(|| AppCommandError::permission_denied("当前企业空间尚未确认"))?;
    Ok((token, org))
}

/// 只读账单不检查可消费余额；缓存身份仍绑定账号会话和企业空间。
pub(crate) async fn iyw_account_usage_credentials_core(
    conn: &DatabaseConnection,
) -> Result<
    Option<(crate::acp::account_credentials::AccountAccessToken, String)>,
    AppCommandError,
> {
    use sha2::{Digest, Sha256};
    let session = fresh_session(conn).await?;
    if workspace_switching() || session.workspace_pending {
        return Err(AppCommandError::permission_denied(
            "空间切换尚未确认，请刷新账号信息",
        ));
    }
    let Some(token) = session.token.and_then(|token| {
        crate::acp::account_credentials::AccountAccessToken::new(token.access_token)
    }) else {
        return Ok(None);
    };
    let identity = if session.session_id.is_empty() {
        token.expose()
    } else {
        &session.session_id
    };
    let scope = format!(
        "{:x}",
        Sha256::digest(
            serde_json::json!([identity, session.org_code])
                .to_string()
                .as_bytes()
        )
    );
    Ok(Some((token, scope)))
}
