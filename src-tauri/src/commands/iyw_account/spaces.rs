use sea_orm::DatabaseConnection;
use serde_json::{json, Value};

use super::{session, types::*, wire, AppCommandError};

pub(super) async fn switch(
    conn: &DatabaseConnection,
    data: Value,
) -> Result<IywAccountProfile, AppCommandError> {
    let org = wire::required(&data, "orgCode")?;
    let current = session::fresh_session(conn).await?;
    let token = current
        .token
        .as_ref()
        .ok_or_else(|| AppCommandError::authentication_failed("请先登录"))?;
    let response = wire::post(
        "/user-service/user/getMyInfo",
        json!({}),
        Some(&token.access_token),
    )
    .await?;
    wire::success(&response, "无法确认当前空间")?;
    let target = response["data"]["orgList"]
        .as_array()
        .and_then(|list| {
            list.iter()
                .find(|item| field(item, "orgCode").as_deref() == Some(&org))
        })
        .ok_or_else(|| AppCommandError::permission_denied("请选择当前账号可用的空间"))?;
    if !workspace(target).is_some_and(|space| space.enabled) {
        return Err(AppCommandError::permission_denied("该空间已停用"));
    }
    if field(&response["data"]["orgInfo"], "orgCode").as_deref() == Some(&org) {
        return super::profile::fetch_profile_with_session(conn, &current, true).await;
    }
    tracing::info!(
        stage = "workspace_switch",
        "[iyw-account] switching workspace"
    );
    let mut next = pending_switch(&current, org.clone());
    session::save_session(conn, &next).await?;
    let outcome = wire::post(
        "/account-admin/revise/switchWorkspace",
        json!({"orgCode":target["orgCode"]}),
        Some(&token.access_token),
    )
    .await;
    match outcome {
        Ok(response) => {
            if let Err(error) = wire::success(&response, "空间切换失败") {
                session::save_session(conn, &current).await?;
                return Err(error);
            }
        }
        Err(error) => return reconcile_uncertain(conn, current, org, error).await,
    }
    refresh_product(conn, &mut next).await?;
    confirmed_profile(conn, &next).await
}

fn pending_switch(current: &StoredSession, org: String) -> StoredSession {
    StoredSession {
        session_id: uuid::Uuid::new_v4().to_string(),
        org_code: Some(org),
        workspace_pending: true,
        business_credentials_pending: true,
        product_token: None,
        balance: None,
        ..current.clone()
    }
}

async fn confirmed_profile(
    conn: &DatabaseConnection,
    session: &StoredSession,
) -> Result<IywAccountProfile, AppCommandError> {
    match super::profile::fetch_profile_with_session(conn, session, true).await {
        Ok(profile) => Ok(profile),
        Err(error) if error.code == super::AppErrorCode::AuthenticationFailed => Err(error),
        Err(error) => {
            tracing::warn!(stage = "workspace_confirmation", code = ?error.code,
                "[iyw-account] workspace needs confirmation before new requests");
            Ok(IywAccountProfile {
                logged_in: true,
                org_code: session.org_code.clone(),
                workspace_pending: true,
                business_credentials_pending: session.business_credentials_pending,
                ..IywAccountProfile::default()
            })
        }
    }
}

async fn reconcile_uncertain(
    conn: &DatabaseConnection,
    mut session: StoredSession,
    org: String,
    error: AppCommandError,
) -> Result<IywAccountProfile, AppCommandError> {
    let token = session.token.as_ref().ok_or_else(|| error.clone())?;
    match super::profile::fetch_profile_with_token(&token.access_token).await {
        Ok(profile) if profile.org_code.as_deref() != Some(&org) => {
            session::save_session(conn, &session).await?;
            return Err(error);
        }
        Ok(_) => {}
        Err(_) => {
            session.workspace_pending = true;
            session.org_code = Some(org);
            session.balance = None;
            session.product_token = None;
            session.business_credentials_pending = true;
            session.session_id = uuid::Uuid::new_v4().to_string();
            session::save_session(conn, &session).await?;
            return confirmed_profile(conn, &session).await;
        }
    }
    session.org_code = Some(org);
    session.session_id = uuid::Uuid::new_v4().to_string();
    session.balance = None;
    session.product_token = None;
    session.business_credentials_pending = true;
    session::save_session(conn, &session).await?;
    refresh_product(conn, &mut session).await?;
    confirmed_profile(conn, &session).await
}

pub(super) async fn refresh_product(
    conn: &DatabaseConnection,
    session: &mut StoredSession,
) -> Result<(), AppCommandError> {
    let token = session
        .token
        .as_ref()
        .ok_or_else(|| AppCommandError::authentication_failed("请先登录"))?;
    let result = wire::post(
        "/account-admin/login/product",
        json!({}),
        Some(&token.access_token),
    )
    .await;
    if let Ok(response) = result.as_ref() {
        if matches!(code(response), Some(1 | 413)) {
            session.product_token = field(&response["data"], "accessTokenProduct");
        }
    }
    session.business_credentials_pending = session.product_token.is_none();
    tracing::info!(
        stage = "workspace_product_token",
        pending = session.business_credentials_pending,
        "[iyw-account] business credentials refreshed"
    );
    session::save_session(conn, session).await
}

pub(super) async fn recover(
    conn: &DatabaseConnection,
) -> Result<IywAccountProfile, AppCommandError> {
    let mut session = session::fresh_session(conn).await?;
    if session.workspace_pending {
        let token = session
            .token
            .as_ref()
            .ok_or_else(|| AppCommandError::authentication_failed("请先登录"))?;
        let profile = super::profile::fetch_profile_with_token(&token.access_token).await?;
        session.org_code = profile.org_code;
        session.workspace_pending = false;
        session.balance = None;
        session::save_session(conn, &session).await?;
    }
    refresh_product(conn, &mut session).await?;
    confirmed_profile(conn, &session).await
}
