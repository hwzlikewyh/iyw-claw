use sea_orm::DatabaseConnection;
use serde_json::{json, Value};

use super::{
    pending::{self, PendingFlow},
    types::*,
    wire, AppCommandError,
};

pub(super) async fn handle(
    conn: &DatabaseConnection,
    response: Value,
    allow_pending: bool,
) -> Result<IywLoginResult, AppCommandError> {
    let code = code(&response);
    let data = response["data"].clone();
    let status = match code {
        Some(409 | 411 | 412) => "no_workspace",
        Some(410) => "workspace_required",
        Some(1) if !data["token"].is_object() && field(&data, "wechatKey").is_some() => {
            "phone_binding_required"
        }
        Some(1) if field(&data, "selectIntent").as_deref() == Some("0") => "identity_required",
        Some(1) if data["token"].is_object() || token(&data).is_some() => {
            return finish(conn, data).await
        }
        Some(1) if field(&data, "wechatKey").is_some() => "phone_binding_required",
        Some(1) if allow_pending => {
            return Ok(IywLoginResult {
                status: "pending".to_string(),
                ..IywLoginResult::default()
            })
        }
        _ => {
            wire::success(&response, "登录失败，请重试")?;
            return Err(AppCommandError::network("Login credentials are missing"));
        }
    };
    let mut flow = PendingFlow::new(data, status);
    if status == "workspace_required" {
        flow.workspaces = principals(&flow.login_data).await?;
    }
    let workspaces = flow.workspaces.iter().filter_map(workspace).collect();
    let flow_id = pending::put(flow).await;
    tracing::info!(
        stage = "login",
        outcome = status,
        "[iyw-account] login requires completion"
    );
    Ok(IywLoginResult {
        status: status.to_string(),
        flow_id: Some(flow_id),
        workspaces,
        message: field(&response, "message"),
        ..IywLoginResult::default()
    })
}

async fn principals(data: &Value) -> Result<Vec<Value>, AppCommandError> {
    let user = data
        .get("userId")
        .ok_or_else(|| AppCommandError::network("Login identity is missing"))?;
    let response = wire::post(
        "/user-service/user/getUserPrincipal",
        json!({"userId":user}),
        None,
    )
    .await?;
    wire::success(&response, "无法加载空间列表")?;
    response["data"]["list"]
        .as_array()
        .cloned()
        .ok_or_else(|| AppCommandError::network("Workspace list is missing"))
}

pub(super) async fn finish(
    conn: &DatabaseConnection,
    data: Value,
) -> Result<IywLoginResult, AppCommandError> {
    let token = token(&data["token"])
        .or_else(|| token(&data))
        .ok_or_else(|| AppCommandError::network("登录凭证缺失，请重新登录"))?;
    let profile = super::profile::fetch_profile_with_token(&token.access_token).await?;
    if data["_scanVersion"]
        .as_u64()
        .is_some_and(|version| !pending::scan_is_current(version))
    {
        return Err(AppCommandError::invalid_input("扫码流程已取消，请重新登录"));
    }
    if profile.org_code.is_none() {
        return Err(AppCommandError::invalid_input(
            "账号尚未选择有效空间，请重新登录",
        ));
    }
    let mut session = StoredSession {
        expires_at_ms: Some(super::session::expires_at(&token.expiration)?),
        token: Some(token),
        session_id: uuid::Uuid::new_v4().to_string(),
        org_code: profile.org_code.clone(),
        business_credentials_pending: true,
        ..StoredSession::default()
    };
    super::session::save_session(conn, &session).await?;
    super::spaces::refresh_product(conn, &mut session).await?;
    crate::acp::account_credentials::sync_existing_agent_credentials(conn).await?;
    let profile = super::profile::fetch_profile_with_session(conn, &session, true).await?;
    crate::commands::skill_market::spawn_background_availability_reconcile(conn.clone());
    tracing::info!(stage = "login", "[iyw-account] account login completed");
    Ok(IywLoginResult {
        status: "authenticated".to_string(),
        profile: Some(profile),
        ..IywLoginResult::default()
    })
}

pub(super) async fn select(
    conn: &DatabaseConnection,
    data: Value,
    create: bool,
) -> Result<IywLoginResult, AppCommandError> {
    let (id, flow) = pending::get(&data).await?;
    if !matches!(flow.status.as_str(), "workspace_required" | "no_workspace") {
        return Err(AppCommandError::invalid_input("请先完成当前登录步骤"));
    }
    let target = if create && !flow.workspaces.is_empty() {
        flow.workspaces[0].clone()
    } else if create {
        let response = wire::post(
            "/user-service/user/createPersonSpace",
            json!({"userId":flow.login_data["userId"]}),
            None,
        )
        .await?;
        wire::success(&response, "创建个人空间失败")?;
        let target = response["data"].clone();
        pending::remember_workspace(&id, target.clone()).await;
        target
    } else {
        let org = wire::required(&data, "orgCode")?;
        flow.workspaces
            .iter()
            .find(|entry| field(entry, "orgCode").as_deref() == Some(&org))
            .cloned()
            .ok_or_else(|| AppCommandError::invalid_input("请选择当前账号可用的空间"))?
    };
    if !workspace(&target).is_some_and(|target| target.enabled) {
        return Err(AppCommandError::permission_denied(
            "该空间已停用，请选择其他空间",
        ));
    }
    let request = json!({"userId":flow.login_data["userId"], "sessionKey":flow.login_data["sessionKey"],
        "caihongId":target.get("caihongNo").or_else(|| target.get("caihongNO")), "employName":target["employName"], "orgCode":target["orgCode"],
        "employeeId":target["employeeId"], "platformType":1, "clientType":1, "orgFlag":target["orgFlag"]});
    let response = wire::post("/account-admin/login/getToken", request, None).await?;
    wire::success(&response, "选择空间失败")?;
    let result = finish(conn, response["data"].clone()).await?;
    pending::remove(&id).await;
    Ok(result)
}

pub(super) async fn identity(
    conn: &DatabaseConnection,
    data: Value,
) -> Result<IywLoginResult, AppCommandError> {
    let (id, flow) = pending::get(&data).await?;
    if flow.status != "identity_required" {
        return Err(AppCommandError::invalid_input("身份选择状态已失效"));
    }
    let identity = data["identity"]
        .as_i64()
        .filter(|value| matches!(value, 0 | 1))
        .ok_or_else(|| AppCommandError::invalid_input("请选择创作身份"))?;
    let token = flow
        .token
        .as_ref()
        .ok_or_else(|| AppCommandError::network("登录凭证缺失"))?;
    let response = wire::post(
        "/account-admin/revise/updateUser",
        json!({"identity":identity,"orgFlag":1}),
        Some(&token.access_token),
    )
    .await?;
    wire::success(&response, "身份设置失败")?;
    let response = wire::post(
        "/user-service/org/focus/save",
        json!({"market":{"type":1}}),
        Some(&token.access_token),
    )
    .await?;
    wire::success(&response, "资料保存失败")?;
    let result = finish(conn, flow.login_data).await?;
    pending::remove(&id).await;
    Ok(result)
}
