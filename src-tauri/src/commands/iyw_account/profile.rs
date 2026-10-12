use sea_orm::DatabaseConnection;
use serde_json::{json, Value};

use super::{types::*, wire, AppCommandError, AppErrorCode};

pub(super) async fn fetch_profile_with_token(
    token: &str,
) -> Result<IywAccountProfile, AppCommandError> {
    let body = wire::post("/user-service/user/getMyInfo", json!({}), Some(token)).await?;
    if matches!(code(&body), Some(409 | 410 | 411 | 412)) {
        return Err(super::account_error(
            field(&body, "message"),
            "账号空间状态已改变，请重新登录选择空间",
        ));
    }
    wire::success(&body, "Unable to load account profile")?;
    let data = &body["data"];
    if !data["userInfo"].is_object() || field(&data["userInfo"], "userId").is_none() {
        return Err(AppCommandError::network("Account profile is missing"));
    }
    let profile = project(data);
    if profile.org_code.is_none() {
        return Err(super::account_error(None, "账号没有可用空间，请重新登录"));
    }
    Ok(profile)
}

fn project(data: &Value) -> IywAccountProfile {
    let user = &data["userInfo"];
    let org = &data["orgInfo"];
    IywAccountProfile {
        logged_in: true,
        user_id: field(user, "userId"),
        name: field(user, "name"),
        nick_name: field(user, "nickName"),
        phone: field(user, "phone").or_else(|| field(user, "phoneNum")),
        avatar_url: super::normalize_asset_url(field(user, "avatar")),
        org_name: field(org, "name"),
        org_logo_url: super::normalize_asset_url(field(org, "logo")),
        org_code: field(org, "orgCode"),
        org_list: data["orgList"]
            .as_array()
            .map(|list| list.iter().filter_map(workspace).collect())
            .unwrap_or_default(),
        ..IywAccountProfile::default()
    }
}

pub(super) async fn fetch_profile_with_session(
    conn: &DatabaseConnection,
    before: &StoredSession,
    include_balance: bool,
) -> Result<IywAccountProfile, AppCommandError> {
    let token = before
        .token
        .as_ref()
        .ok_or_else(|| AppCommandError::authentication_failed("请先登录"))?;
    let mut session = before.clone();
    let mut profile = match fetch_profile_with_token(&token.access_token).await {
        Ok(profile) => profile,
        Err(error) if error.code == AppErrorCode::AuthenticationFailed => {
            session = super::session::renew_session(conn, before).await?;
            let token = session
                .token
                .as_ref()
                .ok_or_else(|| AppCommandError::authentication_failed("请重新登录"))?;
            fetch_profile_with_token(&token.access_token).await?
        }
        Err(error) => return Err(error),
    };
    if !session.workspace_pending && session.org_code != profile.org_code {
        let mut next = session.clone();
        next.session_id = uuid::Uuid::new_v4().to_string();
        next.org_code = profile.org_code.clone();
        next.product_token = None;
        next.business_credentials_pending = true;
        next.balance = None;
        if !super::session::save_if_current(conn, &session, &next).await? {
            return Err(AppCommandError::network("账号空间已改变，请刷新后重试"));
        }
        session = next;
    }
    profile.workspace_pending = session.workspace_pending && session.org_code != profile.org_code;
    profile.business_credentials_pending = session.business_credentials_pending;
    if profile.workspace_pending {
        return Ok(profile);
    }
    if session.workspace_pending {
        let mut next = session.clone();
        next.workspace_pending = false;
        super::session::save_if_current(conn, &session, &next).await?;
        session = next;
    }
    if !super::session::same_session(&session, &super::session::load_session(conn).await?) {
        return Err(AppCommandError::network("账号状态已改变，请刷新后重试"));
    }
    if include_balance {
        super::balance_cache::refresh(conn, &session, &mut profile).await?;
    }
    Ok(profile)
}
