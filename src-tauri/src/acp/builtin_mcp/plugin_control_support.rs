use rmcp::model::CallToolResult;
use rmcp::ErrorData;
use serde_json::{json, Value};

use super::authority::SessionContext;
use super::plugin_control::PluginInstallRequest;

pub(super) struct InstallCandidate {
    pub name: String,
    pub version: String,
    pub permissions: crate::commands::skill_market::SkillPluginPermissions,
}

pub(super) fn plugin_database() -> Result<sea_orm::DatabaseConnection, ErrorData> {
    crate::plugin_runtime::global::database()
        .ok_or_else(|| ErrorData::internal_error("plugin database is unavailable", None))
}

pub(super) fn installed_plugin(
    slug: &str,
) -> Option<crate::plugin_runtime::registry::PluginDescriptor> {
    crate::plugin_runtime::registry::global_snapshot()?
        .plugins
        .get(slug)
        .filter(|plugin| plugin.available)
        .cloned()
}

pub(super) async fn load_install_candidate(
    database: &sea_orm::DatabaseConnection,
    request: &PluginInstallRequest,
) -> Result<InstallCandidate, ErrorData> {
    let detail = crate::commands::skill_market::detail_core(
        database,
        request.skill_id.clone(),
        Some(request.version.clone()),
    )
    .await
    .map_err(|error| ErrorData::invalid_params(error.to_string(), None))?;
    validate_install_request(detail, request)
}

fn validate_install_request(
    detail: crate::commands::skill_market::SkillMarketDetail,
    request: &PluginInstallRequest,
) -> Result<InstallCandidate, ErrorData> {
    let version = &detail.skill.current_version;
    let plugin = version
        .plugin
        .as_ref()
        .filter(|value| value.schema_version >= 2);
    if version.package_type != crate::commands::skill_market::SkillPackageType::Plugin
        || plugin.is_none()
        || detail.skill.publisher_type != "official"
    {
        return Err(ErrorData::invalid_params(
            "only official v2 plugins can be installed",
            None,
        ));
    }
    if request.plugin_name != detail.skill.slug || request.version != version.version {
        return Err(ErrorData::invalid_params(
            "requested plugin identity or version is unavailable",
            None,
        ));
    }
    let permissions = plugin
        .and_then(|value| value.permissions.as_ref())
        .cloned()
        .ok_or_else(|| ErrorData::invalid_params("plugin permissions are missing", None))?;
    Ok(InstallCandidate {
        name: detail.skill.slug.clone(),
        version: version.version.clone(),
        permissions,
    })
}

pub(super) async fn approve_scope(
    database: &sea_orm::DatabaseConnection,
    plugin_slug: &str,
    authority: &SessionContext,
) -> Result<(), ErrorData> {
    let workspace_key = crate::commands::skill_inventory::workspace_key(Some(
        authority.cwd().to_string_lossy().as_ref(),
    ));
    let agent_type = authority.agent_type().as_wire();
    crate::db::service::plugin_runtime_approval_service::approve_plugin(
        database,
        crate::db::service::plugin_runtime_approval_service::PluginApprovalScope {
            plugin_slug,
            workspace_key: &workspace_key,
            agent_type: agent_type.as_ref(),
        },
    )
    .await
    .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
    crate::plugin_runtime::registry::reconcile_global(database)
        .await
        .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
    Ok(())
}

pub(super) fn approved(result: &CallToolResult, label: &str) -> bool {
    result
        .structured_content
        .as_ref()
        .and_then(|value| value.get("answers"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|answer| answer.get("selected").and_then(Value::as_array))
        .flatten()
        .any(|value| value.as_str() == Some(label))
}

pub(super) fn control_result(action: &str) -> CallToolResult {
    CallToolResult::structured(json!({
        "success": true,
        "action": action,
        "catalog_digest": crate::plugin_runtime::registry::global_snapshot()
            .map(|snapshot| snapshot.digest.clone())
            .unwrap_or_default()
    }))
}

pub(super) fn permission_summary(
    permissions: &crate::commands::skill_market::SkillPluginPermissions,
) -> String {
    format!(
        "workspace read [{}], write [{}]; network connect [{}], resource [{}], frame [{}]; host [{}]",
        summarize(&permissions.workspace.read),
        summarize(&permissions.workspace.write),
        summarize(&permissions.network.connect_domains),
        summarize(&permissions.network.resource_domains),
        summarize(&permissions.network.frame_domains),
        summarize(&permissions.host)
    )
}

fn summarize(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values
            .iter()
            .take(12)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    }
}
