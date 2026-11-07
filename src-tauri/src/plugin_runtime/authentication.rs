use super::mcp_client::PluginMcpClient;
use super::types::{PluginInvokeError, RuntimeLaunchSpec};
use crate::commands::skill_market::plugin_setup::{Authentication, ToolAction};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const AUTH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const MAX_ACCOUNT_OPERATIONS: usize = 32;
static OPERATIONS: std::sync::LazyLock<std::sync::Mutex<std::collections::BTreeSet<String>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::BTreeSet::new()));

pub(super) struct AccountOperation(String);
impl Drop for AccountOperation {
    fn drop(&mut self) {
        if let Ok(mut operations) = OPERATIONS.lock() {
            operations.remove(&self.0);
        }
    }
}

pub(super) fn reserve(spec: &RuntimeLaunchSpec) -> Result<AccountOperation, PluginInvokeError> {
    let key = serde_json::json!([spec.install_root, spec.key.connector_key]).to_string();
    let mut operations = OPERATIONS
        .lock()
        .map_err(|_| invalid("插件账号操作状态不可用"))?;
    if operations.len() >= MAX_ACCOUNT_OPERATIONS || !operations.insert(key.clone()) {
        return Err(invalid("此插件连接器已有账号操作正在进行，请稍后重试"));
    }
    Ok(AccountOperation(key))
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthOperation {
    Status,
    Login,
    Logout,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountStatus {
    pub state: String,
    pub authenticated: Option<bool>,
    pub authorization_url: Option<String>,
}

pub(super) fn declaration(
    spec: &RuntimeLaunchSpec,
) -> Result<Option<Authentication>, PluginInvokeError> {
    let plugin = super::registry::global_snapshot()
        .and_then(|snapshot| snapshot.plugins.get(&spec.key.plugin_slug).cloned())
        .ok_or_else(|| invalid("插件已卸载或暂不可用"))?;
    super::setup_store::contract(&plugin, &spec.key.connector_key)
        .map(|preparation| preparation.authentication)
        .map_err(|error| invalid(&error.message))
}

pub(super) async fn execute(
    client: &PluginMcpClient,
    auth: &Authentication,
    operation: AuthOperation,
) -> Result<AccountStatus, PluginInvokeError> {
    match operation {
        AuthOperation::Status => status(client, auth).await,
        AuthOperation::Login => login(client, auth).await,
        AuthOperation::Logout => {
            let action = auth
                .logout
                .as_ref()
                .ok_or_else(|| invalid("发布者未声明账号断开工具"))?;
            call(client, action).await?;
            let result = status(client, auth).await?;
            if result.authenticated != Some(false) {
                return Err(invalid("插件仍报告账号已连接，请检查发布者断开工具"));
            }
            Ok(AccountStatus {
                state: "disconnected".into(),
                ..result
            })
        }
    }
}

async fn status(
    client: &PluginMcpClient,
    auth: &Authentication,
) -> Result<AccountStatus, PluginInvokeError> {
    let result = tokio::time::timeout(AUTH_TIMEOUT, client.read_only_tool(&auth.status))
        .await
        .map_err(|_| invalid("插件账号状态检查超时"))??;
    let value = result
        .pointer(&auth.authenticated_pointer)
        .and_then(Value::as_bool)
        .ok_or_else(|| invalid("插件账号状态未返回声明的布尔字段，请联系发布者修正声明"))?;
    Ok(AccountStatus {
        state: if value {
            "authenticated"
        } else {
            "authentication_required"
        }
        .into(),
        authenticated: Some(value),
        authorization_url: None,
    })
}

async fn login(
    client: &PluginMcpClient,
    auth: &Authentication,
) -> Result<AccountStatus, PluginInvokeError> {
    let action = auth
        .login
        .as_ref()
        .ok_or_else(|| invalid("发布者未声明登录工具，请使用插件自身界面登录"))?;
    let result = call(client, action).await?;
    let url = if auth.authorization_url_pointer.is_empty() {
        None
    } else {
        let raw = result
            .pointer(&auth.authorization_url_pointer)
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("登录工具没有返回声明的授权地址"))?;
        Some(authorization_url(raw)?)
    };
    Ok(AccountStatus {
        state: "login_started".into(),
        authenticated: None,
        authorization_url: url,
    })
}

fn authorization_url(raw: &str) -> Result<String, PluginInvokeError> {
    let url = reqwest::Url::parse(raw).map_err(|_| invalid("插件授权地址无效"))?;
    let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    if (url.scheme() != "https" && !(url.scheme() == "http" && loopback))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.host_str().is_none()
    {
        return Err(invalid(
            "插件授权地址必须使用HTTPS或本机回调地址，且不能嵌入凭据",
        ));
    }
    Ok(url.into())
}

async fn call(client: &PluginMcpClient, action: &ToolAction) -> Result<Value, PluginInvokeError> {
    let result = tokio::time::timeout(
        AUTH_TIMEOUT,
        client.call_tool(action.tool_name.clone(), action.arguments.clone()),
    )
    .await
    .map_err(|_| invalid("插件账号操作超时，请检查账号状态后再决定是否重试"))?
    .map_err(|_| invalid("插件账号操作失败，请检查服务及发布者配置"))?;
    if result.is_error == Some(true) {
        return Err(invalid("插件账号操作未成功，请检查服务及发布者配置"));
    }
    serde_json::to_value(result).map_err(|_| invalid("插件账号响应格式无效"))
}

impl PluginMcpClient {
    async fn read_only_tool(&self, action: &ToolAction) -> Result<Value, PluginInvokeError> {
        let tools = self
            .peer
            .list_all_tools()
            .await
            .map_err(|_| invalid("无法读取插件状态工具定义"))?;
        if !tools.iter().any(|tool| {
            tool.name == action.tool_name
                && tool
                    .annotations
                    .as_ref()
                    .is_some_and(|annotations| annotations.read_only_hint == Some(true))
        }) {
            return Err(invalid("插件实际状态工具未声明只读，已停止账号检查"));
        }
        call(self, action).await
    }
}

pub(super) async fn require(
    client: &PluginMcpClient,
    spec: &RuntimeLaunchSpec,
    tool: &str,
) -> Result<(), PluginInvokeError> {
    let Some(auth) = declaration(spec)? else {
        return Ok(());
    };
    let account_tool = [
        Some(&auth.status),
        auth.login.as_ref(),
        auth.logout.as_ref(),
    ]
    .into_iter()
    .flatten()
    .any(|action| action.tool_name == tool);
    if account_tool {
        return Ok(());
    }
    let _operation = reserve(spec)?;
    if status(client, &auth).await?.authenticated != Some(true) {
        return Err(PluginInvokeError::before_effect(
            "plugin_authentication_required",
            "插件业务账号未登录或授权已失效，请先在插件准备中连接账号",
        ));
    }
    Ok(())
}

fn invalid(message: &str) -> PluginInvokeError {
    PluginInvokeError::before_effect("plugin_authentication_failed", message)
}
