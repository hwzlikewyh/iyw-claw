use sea_orm::DatabaseConnection;

use super::{types::*, AppCommandError};

pub(super) const FAILURE_LIMIT: u8 = 10;

pub(super) async fn refresh(
    conn: &DatabaseConnection,
    session: &StoredSession,
    profile: &mut IywAccountProfile,
) -> Result<(), AppCommandError> {
    let scope = format!(
        "{}:{}",
        profile.user_id.as_deref().unwrap_or_default(),
        profile.org_code.as_deref().unwrap_or_default()
    );
    let mut cache = session
        .balance
        .clone()
        .filter(|cache| cache.scope == scope)
        .unwrap_or_else(|| BalanceCache {
            scope,
            ..BalanceCache::default()
        });
    let token = session
        .token
        .as_ref()
        .ok_or_else(|| AppCommandError::authentication_failed("请先登录"))?;
    match super::points::fetch(&super::wire::http_client()?, &token.access_token).await {
        Ok(amount) => {
            cache.amount = Some(amount);
            cache.failures = 0;
        }
        Err(error) => {
            cache.failures = cache.failures.saturating_add(1).min(FAILURE_LIMIT);
            tracing::warn!(stage = "points", failures = cache.failures, has_cached_balance = cache.amount.is_some(),
                code = ?error.code, "[iyw-account] points unavailable; preserving last confirmed balance");
        }
    }
    profile.balance_points = cache.amount;
    profile.points_refresh_failures = cache.failures;
    profile.points_stale = cache.failures > 0;
    profile.points_requests_paused = cache.failures >= FAILURE_LIMIT;
    let mut next = session.clone();
    next.org_code = profile.org_code.clone();
    next.balance = Some(cache);
    if !super::session::save_if_current(conn, session, &next).await? {
        return Err(AppCommandError::network("账号状态已改变，请刷新后重试"));
    }
    Ok(())
}

pub(crate) async fn ensure_can_consume(conn: &DatabaseConnection) -> Result<(), AppCommandError> {
    if super::session::workspace_switching() {
        return Err(AppCommandError::permission_denied(
            "空间正在切换，请稍后再发起新请求",
        ));
    }
    let session = super::session::load_session(conn).await?;
    if session.workspace_pending {
        return Err(AppCommandError::permission_denied(
            "空间切换尚未确认，请刷新账号信息后重试",
        ));
    }
    if session
        .balance
        .as_ref()
        .is_some_and(|cache| cache.failures >= FAILURE_LIMIT)
    {
        return Err(AppCommandError::permission_denied(
            "点数连续刷新失败，请刷新点数后再发送新请求",
        ));
    }
    Ok(())
}
