use base64::Engine;
use rmcp::model::CallToolResult;
use rmcp::ErrorData;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::super::authority::SessionContext;
use super::super::delivery::RelayDelivery;
use super::super::handler::BuiltinMcpHandler;
use super::super::invocation::ensure_active;
use super::{cancelled, command_error, request_confirmation, success};

pub(super) async fn plugin(
    handler: &BuiltinMcpHandler,
    authority: SessionContext,
    delivery: Option<RelayDelivery>,
    request_id: serde_json::Value,
    request_cancel: CancellationToken,
    mut request: crate::commands::skill_market::SkillMarketPublishRequest,
) -> Result<CallToolResult, ErrorData> {
    validate_connector_package(&request.files)?;
    let bytes = request
        .files
        .iter()
        .fold(0_u64, |total, file| total.saturating_add(file.size));
    let question = format!(
        "Publish plugin '{}' version {} to the {} market? The package contains {} files ({} bytes).",
        request.slug, request.version, request.visibility, request.files.len(), bytes
    );
    if !request_confirmation(
        handler,
        authority.clone(),
        delivery,
        request_id,
        request_cancel.clone(),
        question,
        "Publish",
    )
    .await?
    {
        return Ok(cancelled("publish_plugin"));
    }
    ensure_active(&authority, &request_cancel)?;
    request.package_type = crate::commands::skill_market::SkillPackageType::Plugin;
    let detail =
        crate::commands::skill_market::publish_core(&handler.gateway_service().conn, request)
            .await
            .map_err(command_error)?;
    Ok(success(
        "publish_plugin",
        json!({"id": detail.skill.id, "slug": detail.skill.slug, "version": detail.skill.current_version.version}),
    ))
}

pub(super) async fn version(
    handler: &BuiltinMcpHandler,
    authority: SessionContext,
    delivery: Option<RelayDelivery>,
    request_id: serde_json::Value,
    request_cancel: CancellationToken,
    mut request: crate::commands::skill_market::SkillMarketAddVersionRequest,
) -> Result<CallToolResult, ErrorData> {
    validate_connector_package(&request.files)?;
    let bytes = request
        .files
        .iter()
        .fold(0_u64, |total, file| total.saturating_add(file.size));
    let question = format!(
        "Publish version {} of plugin '{}' with {} files ({} bytes)?",
        request.version,
        request.id,
        request.files.len(),
        bytes
    );
    if !request_confirmation(
        handler,
        authority.clone(),
        delivery,
        request_id,
        request_cancel.clone(),
        question,
        "Publish version",
    )
    .await?
    {
        return Ok(cancelled("publish_plugin_version"));
    }
    ensure_active(&authority, &request_cancel)?;
    request.package_type = crate::commands::skill_market::SkillPackageType::Plugin;
    let detail =
        crate::commands::skill_market::add_version_core(&handler.gateway_service().conn, request)
            .await
            .map_err(command_error)?;
    Ok(success(
        "publish_plugin_version",
        json!({"id": detail.skill.id, "slug": detail.skill.slug, "version": detail.skill.current_version.version}),
    ))
}

pub(super) async fn update_metadata(
    handler: &BuiltinMcpHandler,
    authority: SessionContext,
    delivery: Option<RelayDelivery>,
    request_id: serde_json::Value,
    request_cancel: CancellationToken,
    request: crate::commands::skill_market::SkillMarketMetadataRequest,
) -> Result<CallToolResult, ErrorData> {
    require_connector_market_item(&handler.gateway_service().conn, &request.id).await?;
    let question = format!("Update market metadata for item '{}' ?", request.id);
    if !request_confirmation(
        handler,
        authority.clone(),
        delivery,
        request_id,
        request_cancel.clone(),
        question,
        "Update metadata",
    )
    .await?
    {
        return Ok(cancelled("update_market_metadata"));
    }
    ensure_active(&authority, &request_cancel)?;
    let detail = crate::commands::skill_market::update_metadata_core(
        &handler.gateway_service().conn,
        request,
    )
    .await
    .map_err(command_error)?;
    Ok(success(
        "update_market_metadata",
        json!({"id": detail.skill.id, "slug": detail.skill.slug}),
    ))
}

pub(super) async fn delete(
    handler: &BuiltinMcpHandler,
    authority: SessionContext,
    delivery: Option<RelayDelivery>,
    request_id: serde_json::Value,
    request_cancel: CancellationToken,
    market_id: String,
) -> Result<CallToolResult, ErrorData> {
    require_connector_market_item(&handler.gateway_service().conn, &market_id).await?;
    let question = format!("Permanently delete market item '{market_id}'?");
    if !request_confirmation(
        handler,
        authority.clone(),
        delivery,
        request_id,
        request_cancel.clone(),
        question,
        "Delete market item",
    )
    .await?
    {
        return Ok(cancelled("delete_market_item"));
    }
    ensure_active(&authority, &request_cancel)?;
    crate::commands::skill_market::delete_core(&handler.gateway_service().conn, market_id.clone())
        .await
        .map_err(command_error)?;
    Ok(success("delete_market_item", json!({"id": market_id})))
}

fn validate_connector_package(
    files: &[crate::commands::skill_market::SkillMarketUploadFile],
) -> Result<(), ErrorData> {
    let file = files
        .iter()
        .find(|file| {
            matches!(
                file.path.as_str(),
                ".iyw-plugin.json" | "./.iyw-plugin.json"
            )
        })
        .ok_or_else(|| ErrorData::invalid_params("插件包缺少 .iyw-plugin.json", None))?;
    if file.size > 1024 * 1024 {
        return Err(ErrorData::invalid_params("插件清单超过 1 MiB", None));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&file.content_base64)
        .map_err(|_| ErrorData::invalid_params("插件清单不是有效 Base64", None))?;
    if bytes.len() as u64 != file.size {
        return Err(ErrorData::invalid_params("插件清单大小不匹配", None));
    }
    let manifest: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| ErrorData::invalid_params("插件清单不是有效 JSON", None))?;
    if !manifest["components"]["connectors"]
        .as_array()
        .is_some_and(|connectors| !connectors.is_empty())
    {
        return Err(ErrorData::invalid_params(
            "插件清单没有声明任何连接器",
            None,
        ));
    }
    Ok(())
}

async fn require_connector_market_item(
    database: &sea_orm::DatabaseConnection,
    market_id: &str,
) -> Result<(), ErrorData> {
    let detail = crate::commands::skill_market::detail_core(database, market_id.into(), None)
        .await
        .map_err(command_error)?;
    let is_connector_plugin = detail.skill.current_version.package_type
        == crate::commands::skill_market::SkillPackageType::Plugin
        && detail
            .skill
            .current_version
            .plugin
            .as_ref()
            .is_some_and(|plugin| {
                plugin
                    .components
                    .iter()
                    .any(|component| component.kind == "connector")
            });
    if !is_connector_plugin {
        return Err(ErrorData::invalid_params(
            "目标市场项目不是包含连接器的插件",
            None,
        ));
    }
    Ok(())
}
