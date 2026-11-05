use rmcp::model::CallToolResult;
use rmcp::ErrorData;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::super::authority::SessionContext;
use super::super::delivery::RelayDelivery;
use super::super::handler::BuiltinMcpHandler;
use super::super::invocation::ensure_active;
use super::{cancelled, command_error, request_confirmation, success};

const PROVIDER: &str = "iyw_gateway";

pub(super) async fn installed_list(
    service: &super::super::iyw_service::IywGatewayService,
) -> Result<CallToolResult, ErrorData> {
    let items = crate::commands::mcp::mcp_scan_local_core(&service.conn)
        .await
        .map_err(command_error)?
        .into_iter()
        .filter(|item| item.spec.get("iyw_builtin").is_none())
        .filter(|item| item.id != "iyw-capability-gateway")
        .map(|item| {
            let id = item.id;
            let name = item.display_name.unwrap_or_else(|| id.clone());
            json!({
                "id": id,
                "name": name,
                "protocol": item.spec["type"].as_str().unwrap_or("stdio"),
                "app_managed_global_enabled": item.enabled,
                "missing_configuration": item.missing_config,
                "plugin_managed": !item.sources.is_empty()
            })
        })
        .collect::<Vec<_>>();
    Ok(success("installed_list", json!({"connectors": items})))
}

pub(super) async fn search(
    service: &super::super::iyw_service::IywGatewayService,
    query: Option<String>,
    provider_id: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<CallToolResult, ErrorData> {
    let items = crate::commands::mcp::mcp_search_marketplace_core(
        &service.conn,
        crate::commands::mcp::McpMarketplaceSearch {
            provider_id: provider_id.unwrap_or_else(|| PROVIDER.into()),
            query,
            limit,
            offset,
        },
    )
    .await
    .map_err(command_error)?;
    Ok(success("market_search", json!({"connectors": items})))
}

pub(super) async fn detail(
    service: &super::super::iyw_service::IywGatewayService,
    provider_id: String,
    server_id: String,
) -> Result<CallToolResult, ErrorData> {
    let value = crate::commands::mcp::mcp_get_marketplace_server_detail_core(
        &service.conn,
        provider_id,
        server_id,
    )
    .await
    .map_err(command_error)?;
    let options = value
        .install_options
        .iter()
        .map(|option| {
            let install = &option.spec["iyw_install"];
            json!({
                "id": option.id,
                "protocol": option.protocol,
                "label": option.label,
                "description": option.description,
                "auth_mode": install["auth_mode"],
                "downloads_artifact": install["artifact"],
                "parameters": option.parameters.iter().map(|field| json!({
                    "key": field.key,
                    "label": field.label,
                    "required": field.required,
                    "secret": field.secret,
                    "kind": field.kind,
                    "description": field.description,
                    "enum_values": field.enum_values,
                    "has_default": field.default_value.is_some()
                })).collect::<Vec<_>>()
            })
        })
        .collect::<Vec<_>>();
    Ok(success(
        "market_detail",
        json!({
            "provider_id": value.provider_id,
            "server_id": value.server_id,
            "name": value.name,
            "description": value.description,
            "homepage": value.homepage,
            "latest_version": value.latest_version,
            "install_options": options
        }),
    ))
}

pub(super) async fn install(
    handler: &BuiltinMcpHandler,
    authority: SessionContext,
    delivery: Option<RelayDelivery>,
    request_id: Value,
    request_cancel: CancellationToken,
    input: (String, String, String, String, Option<Value>),
) -> Result<CallToolResult, ErrorData> {
    let (provider, server, option_id, protocol, parameters) = input;
    let service = handler.gateway_service();
    let detail = crate::commands::mcp::mcp_get_marketplace_server_detail_core(
        &service.conn,
        provider.clone(),
        server.clone(),
    )
    .await
    .map_err(command_error)?;
    let option = detail
        .install_options
        .iter()
        .find(|option| option.id == option_id && option.protocol == protocol)
        .ok_or_else(|| ErrorData::invalid_params("市场接入方式不存在", None))?;
    validate_credentials(option, parameters.as_ref())?;
    let question = format!(
        "Add connector '{}' using {}? This may download and run connector code.",
        detail.name, option.label
    );
    if !request_confirmation(
        handler,
        authority.clone(),
        delivery,
        request_id,
        request_cancel.clone(),
        question,
        "Add connector",
    )
    .await?
    {
        return Ok(cancelled("install_market"));
    }
    ensure_active(&authority, &request_cancel)?;
    let installed = crate::commands::mcp::mcp_install_from_marketplace_core(
        &service.conn,
        provider,
        server,
        None,
        Some(option_id),
        Some(protocol),
        parameters,
    )
    .await
    .map_err(command_error)?;
    let requires_oauth = installed.spec["iyw_market"]["auth_mode"].as_str() == Some("oauth");
    Ok(success(
        "install_market",
        json!({
            "id": installed.id,
            "name": installed.display_name,
            "requires_oauth_in_ui": requires_oauth,
            "missing_configuration": installed.missing_config,
            "reconnect_required": true
        }),
    ))
}

fn validate_credentials(
    option: &crate::commands::mcp::McpMarketplaceInstallOption,
    parameters: Option<&Value>,
) -> Result<(), ErrorData> {
    let values = parameters.and_then(Value::as_object);
    let oauth = option.spec["iyw_install"]["auth_mode"].as_str() == Some("oauth");
    let secret_required = option.parameters.iter().any(|field| {
        field.secret
            && (field.required || values.is_some_and(|values| values.contains_key(&field.key)))
    });
    let sensitive_value = option.parameters.iter().any(|field| {
        values.is_some_and(|values| values.contains_key(&field.key)) && sensitive_name(&field.key)
    });
    if oauth || secret_required || sensitive_value {
        return Err(ErrorData::invalid_params(
            "此接入方式需要 OAuth 或敏感凭据，请在连接器市场界面中完成授权和配置",
            Some(json!({"code": "connector_ui_configuration_required"})),
        ));
    }
    Ok(())
}

fn sensitive_name(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    [
        "token",
        "secret",
        "password",
        "api_key",
        "apikey",
        "credential",
    ]
    .iter()
    .any(|part| value.contains(part))
}

pub(super) async fn uninstall(
    handler: &BuiltinMcpHandler,
    authority: SessionContext,
    delivery: Option<RelayDelivery>,
    request_id: Value,
    request_cancel: CancellationToken,
    server_id: String,
) -> Result<CallToolResult, ErrorData> {
    let question = format!("Uninstall connector '{server_id}' from this iyw-claw installation?");
    if !request_confirmation(
        handler,
        authority.clone(),
        delivery,
        request_id,
        request_cancel.clone(),
        question,
        "Uninstall",
    )
    .await?
    {
        return Ok(cancelled("uninstall"));
    }
    ensure_active(&authority, &request_cancel)?;
    crate::commands::mcp::mcp_remove_server_core(
        &handler.gateway_service().conn,
        server_id.clone(),
    )
    .await
    .map_err(command_error)?;
    Ok(success(
        "uninstall",
        json!({"id": server_id, "reconnect_required": true}),
    ))
}

pub(super) async fn set_global_enabled(
    handler: &BuiltinMcpHandler,
    authority: SessionContext,
    delivery: Option<RelayDelivery>,
    request_id: Value,
    request_cancel: CancellationToken,
    server_id: String,
    enabled: bool,
) -> Result<CallToolResult, ErrorData> {
    let installed = crate::commands::mcp::mcp_scan_local_core(&handler.gateway_service().conn)
        .await
        .map_err(command_error)?
        .into_iter()
        .find(|item| item.id == server_id)
        .ok_or_else(|| ErrorData::invalid_params("未检测到该连接器", None))?;
    if !installed.sources.is_empty() {
        return Err(ErrorData::invalid_params(
            "插件连接器请在插件管理中启用或停用",
            Some(json!({"code": "plugin_connector_managed_by_plugin"})),
        ));
    }
    let action = if enabled { "Enable" } else { "Disable" };
    let question = format!("{action} connector '{server_id}' globally for Agent configurations?");
    if !request_confirmation(
        handler,
        authority.clone(),
        delivery,
        request_id,
        request_cancel.clone(),
        question,
        action,
    )
    .await?
    {
        return Ok(cancelled("set_global_enabled"));
    }
    ensure_active(&authority, &request_cancel)?;
    crate::commands::mcp::mcp_set_server_enabled_core(
        &handler.gateway_service().conn,
        server_id.clone(),
        enabled,
    )
    .await
    .map_err(command_error)?;
    Ok(success(
        "set_global_enabled",
        json!({"id": server_id, "enabled": enabled, "reconnect_required": true}),
    ))
}
