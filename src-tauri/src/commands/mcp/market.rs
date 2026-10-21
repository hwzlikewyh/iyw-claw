use sea_orm::DatabaseConnection;
use serde_json::{json, Value};

use super::market_types::{Document, OptionSpec};
use super::{McpMarketplaceItem, McpMarketplaceServerDetail};
use crate::app_error::AppCommandError;

pub(super) const PROVIDER: &str = "iyw_gateway";
const DEFAULT_BASE: &str = "https://gateway.iyw.cn/iyw-fusion-mcp-gateway/api/connector-market";
const MAX_RESPONSE: usize = 8 * 1024 * 1024;

pub(super) async fn require_market_slot(
    conn: &DatabaseConnection,
    id: &str,
) -> Result<Option<String>, AppCommandError> {
    let catalog =
        crate::commands::mcp_catalog::load_or_import(conn, super::scan_legacy_server_specs).await?;
    if let Some(entry) = catalog.servers.get(id) {
        if entry.spec["iyw_market"]["id"].as_str() != Some(id) {
            return Err(AppCommandError::invalid_input(
                "存在同名自定义连接器，请先重命名后再安装",
            ));
        }
        return Ok(entry.spec["iyw_secret_ref"].as_str().map(str::to_owned));
    }
    Ok(None)
}

pub(super) fn base_url() -> Result<reqwest::Url, AppCommandError> {
    let raw =
        std::env::var("IYW_CONNECTOR_MARKET_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE.to_string());
    let url = reqwest::Url::parse(&(raw.trim_end_matches('/').to_string() + "/"))
        .map_err(|_| AppCommandError::configuration_invalid("连接器市场地址无效"))?;
    let loopback = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
    if (url.scheme() != "https" && !(url.scheme() == "http" && loopback))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
    {
        return Err(AppCommandError::configuration_invalid(
            "连接器市场必须使用 HTTPS，开发环境可使用 loopback HTTP",
        ));
    }
    Ok(url)
}

pub(super) fn client() -> Result<reqwest::Client, AppCommandError> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|_| AppCommandError::network("无法创建连接器市场连接"))
}

pub(super) async fn request(
    conn: &DatabaseConnection,
    path: &str,
    body: Value,
) -> Result<Value, AppCommandError> {
    let token = crate::commands::iyw_account::iyw_account_access_token_core(conn)
        .await?
        .ok_or_else(|| AppCommandError::permission_denied("请先登录爱原物账号"))?;
    let url = base_url()?
        .join(path)
        .map_err(|_| AppCommandError::configuration_invalid("连接器市场路径无效"))?;
    let response = client()?
        .post(url)
        .header("token", token.expose())
        .json(&body)
        .send()
        .await
        .map_err(|_| AppCommandError::network("连接器市场暂时无法访问"))?;
    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND && path == "list" {
        tracing::warn!(
            route = "connector-market/list",
            "[MCP] 市场目录路由不存在，请检查网关、控制面部署和入口转发"
        );
        return Err(AppCommandError::network("连接器市场接口不存在（HTTP 404）。请确认 MCP 网关和管理 API 已更新，并检查市场接口转发。"));
    }
    if !status.is_success() {
        return Err(status_error(status.as_u16()));
    }
    let bytes = limited_response(response, MAX_RESPONSE).await?;
    let envelope: Value = serde_json::from_slice(&bytes)
        .map_err(|_| AppCommandError::network("连接器市场响应格式无效"))?;
    response_data(envelope)
}

fn response_data(envelope: Value) -> Result<Value, AppCommandError> {
    if let Some(code) = envelope.get("code").and_then(Value::as_i64) {
        if matches!(code, 401 | 403) {
            return Err(AppCommandError::permission_denied(
                "登录状态失效或无权访问连接器市场，请重新登录",
            ));
        }
        if !matches!(code, 0 | 1 | 200) {
            return Err(AppCommandError::network(format!(
                "连接器市场入口返回失败（code {code}）"
            )));
        }
    }
    envelope
        .get("data")
        .cloned()
        .ok_or_else(|| AppCommandError::network("连接器市场响应缺少 data"))
}

fn status_error(status: u16) -> AppCommandError {
    if matches!(status, 401 | 403) {
        AppCommandError::permission_denied("登录状态失效或无权访问连接器市场，请重新登录")
    } else {
        AppCommandError::network(format!("连接器市场请求失败（HTTP {status}）"))
    }
}

pub(super) async fn limited_response(
    mut response: reqwest::Response,
    limit: usize,
) -> Result<Vec<u8>, AppCommandError> {
    if response
        .content_length()
        .is_some_and(|size| size > limit as u64)
    {
        return Err(AppCommandError::network("连接器市场响应超过大小限制"));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| AppCommandError::network("连接器市场下载中断"))?
    {
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err(AppCommandError::network("连接器市场响应超过大小限制"));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub(super) async fn search(
    conn: &DatabaseConnection,
    input: &super::McpMarketplaceSearch,
) -> Result<Vec<McpMarketplaceItem>, AppCommandError> {
    const DEFAULT_PAGE_SIZE: u32 = 20;
    const MAX_PAGE_SIZE: u32 = 50;
    let limit = input
        .limit
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE);
    let offset = input.offset.unwrap_or_default();
    if offset > 1_000_000 {
        return Err(AppCommandError::invalid_input("连接器分页位置无效"));
    }
    let raw = request(
        conn,
        "list",
        json!({"offset":offset,"limit":limit,"query":input.query.as_deref().unwrap_or("").trim()}),
    )
    .await?;
    let docs: Vec<Document> =
        serde_json::from_value(raw).map_err(|_| AppCommandError::network("连接器目录格式无效"))?;
    Ok(docs.iter().take(limit as usize).map(item).collect())
}

fn item(doc: &Document) -> McpMarketplaceItem {
    McpMarketplaceItem {
        provider_id: PROVIDER.into(),
        server_id: doc.server_id.clone(),
        name: doc.name.clone(),
        description: doc.description.clone(),
        homepage: Some(doc.homepage.clone()),
        remote: doc
            .install_options
            .iter()
            .any(|option| option.protocol != "stdio"),
        verified: false,
        icon_url: Some(doc.icon_url.clone()),
        latest_version: Some(doc.latest_version.clone()),
        protocols: doc
            .install_options
            .iter()
            .map(|option| option.protocol.clone())
            .collect(),
        owner: Some(doc.owner.clone()),
        namespace: None,
        downloads: None,
        score: None,
        is_deployed: None,
    }
}

pub(super) async fn detail(
    conn: &DatabaseConnection,
    id: &str,
) -> Result<McpMarketplaceServerDetail, AppCommandError> {
    let raw = request(conn, "detail", json!({"id":id})).await?;
    let doc: Document =
        serde_json::from_value(raw).map_err(|_| AppCommandError::network("连接器详情格式无效"))?;
    let value = item(&doc);
    let options = doc
        .install_options
        .iter()
        .filter(|option| supported_platform(option))
        .map(project_option)
        .collect::<Vec<_>>();
    Ok(McpMarketplaceServerDetail {
        provider_id: PROVIDER.into(),
        server_id: value.server_id,
        name: value.name,
        description: if doc.instructions.is_empty() {
            value.description
        } else {
            format!("{}\n\n{}", value.description, doc.instructions)
        },
        homepage: value.homepage,
        remote: value.remote,
        verified: false,
        icon_url: value.icon_url,
        latest_version: value.latest_version,
        protocols: value.protocols,
        owner: value.owner,
        namespace: None,
        downloads: None,
        score: None,
        is_deployed: None,
        default_option_id: options.first().map(|option| option.id.clone()),
        spec: options
            .first()
            .map(|option| option.spec.clone())
            .unwrap_or(json!({})),
        install_options: options,
    })
}

pub(super) fn supported_platform(option: &OptionSpec) -> bool {
    let platform = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
    option.platforms.is_empty()
        || option
            .platforms
            .iter()
            .any(|value| value == &platform || value == std::env::consts::OS)
}

fn project_option(option: &OptionSpec) -> super::McpMarketplaceInstallOption {
    let mut spec = option.spec.clone();
    spec["iyw_install"] = json!({"runtime":option.runtime,"artifact":!option.artifact_id.is_empty(),"auth_mode":option.auth_mode,"protocol_versions":option.protocol_versions});
    super::McpMarketplaceInstallOption {
        id: option.id.clone(),
        protocol: option.protocol.clone(),
        label: option.label.clone(),
        description: Some(option.description.clone()),
        spec,
        parameters: option
            .parameters
            .iter()
            .map(|field| super::McpMarketplaceInstallParameter {
                key: field.key.clone(),
                label: field.label.clone(),
                description: Some(field.description.clone()),
                required: field.required,
                secret: field.secret,
                kind: field.kind.clone(),
                default_value: field.default_value.clone(),
                placeholder: Some(field.placeholder.clone()),
                enum_values: field.enum_values.clone(),
                location: None,
            })
            .collect(),
    }
}
