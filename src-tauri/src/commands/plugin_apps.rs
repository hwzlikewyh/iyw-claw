#[path = "plugin_app_catalog.rs"]
mod plugin_app_catalog;
#[path = "plugin_app_context.rs"]
mod plugin_app_context;
#[path = "plugin_app_messages.rs"]
mod plugin_app_messages;
#[path = "plugin_app_open.rs"]
mod plugin_app_open;
#[path = "plugin_app_resources.rs"]
mod plugin_app_resources;
#[path = "plugin_app_types.rs"]
mod plugin_app_types;
pub use plugin_app_catalog::{catalog_core, AppChoice, CatalogRequest, LaunchRequest};
#[path = "plugin_app_launch.rs"]
mod plugin_app_launch;
pub use plugin_app_launch::launch_core;
#[path = "plugin_app_sync.rs"]
mod plugin_app_sync;
pub use plugin_app_sync::{sync_core, SyncRequest, SyncUpdate};

use crate::app_error::AppCommandError;

#[path = "plugin_app_resource_meta.rs"]
mod plugin_app_resource_meta;
#[path = "plugin_app_tools.rs"]
mod plugin_app_tools;
pub(crate) use plugin_app_open::open_core;
pub use plugin_app_resource_meta::{
    PluginAppResourceCsp, PluginAppResourceMeta, PluginAppResourcePermissions,
};
pub use plugin_app_types::{
    PluginAppErrorState, PluginAppMessageRequest, PluginAppMessageResponse, PluginAppOpenRequest,
    PluginAppOpenResponse, PluginAppRpcError, PluginAppTeardownRequest,
};

pub async fn message_core(
    db: &crate::db::AppDatabase,
    apps: &crate::plugin_runtime::app_host::PluginAppRegistry,
    router: &crate::plugin_runtime::router::PluginRouter,
    manager: &crate::acp::manager::ConnectionManager,
    request: PluginAppMessageRequest,
) -> Result<PluginAppMessageResponse, AppCommandError> {
    let params = request.params.unwrap_or(serde_json::Value::Null);
    let bytes = serde_json::to_vec(&params)
        .map_err(|error| AppCommandError::invalid_input(error.to_string()))?
        .len();
    if let Err(error) = apps.authorize_message(
        &request.instance_id,
        &request.lease_token,
        &request.nonce,
        &request.method,
        bytes,
    ) {
        return Ok(plugin_error_response(error));
    }
    if let Err(error) =
        plugin_app_tools::authorize_app_message(db, &request.instance_id, &request.method).await
    {
        return Ok(plugin_error_response(error));
    }
    let result = if request.method == "tools/call" {
        match plugin_app_tools::route_app_tool(db, router, &request.instance_id, params).await {
            Ok(result) => Some(result),
            Err(error) => return Ok(plugin_error_response(error)),
        }
    } else if request.method == "resources/list" {
        match plugin_app_resources::route_app_resources(db, &request.instance_id).await {
            Ok(result) => Some(result),
            Err(error) => return Ok(plugin_error_response(error)),
        }
    } else if request.method == "resources/read" {
        match plugin_app_resources::route_app_resource(db, router, &request.instance_id, params)
            .await
        {
            Ok(result) => Some(result),
            Err(error) => return Ok(plugin_error_response(error)),
        }
    } else if request.method == "ui/message" {
        if let Err(error) =
            plugin_app_messages::send_app_message(db, manager, &request.instance_id, params).await
        {
            return Ok(plugin_error_response(error));
        }
        Some(serde_json::json!({}))
    } else {
        None
    };
    Ok(PluginAppMessageResponse {
        accepted: true,
        result,
        error: None,
    })
}

fn plugin_error_response(
    error: crate::plugin_runtime::types::PluginInvokeError,
) -> PluginAppMessageResponse {
    let code = if error.code.contains("invalid") {
        -32602
    } else if error.code.contains("unsupported") {
        -32601
    } else {
        -32000
    };
    PluginAppMessageResponse {
        accepted: false,
        result: None,
        error: Some(PluginAppRpcError {
            code,
            message: error.message,
        }),
    }
}

pub async fn teardown_core(
    db: &crate::db::AppDatabase,
    apps: &crate::plugin_runtime::app_host::PluginAppRegistry,
    request: PluginAppTeardownRequest,
) -> Result<(), AppCommandError> {
    apps.teardown(&request.instance_id);
    crate::db::service::plugin_app_instance_service::mark_inactive(
        &db.conn,
        &request.instance_id,
        request.conversation_id,
    )
    .await
    .map_err(AppCommandError::db)?;
    Ok(())
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn plugin_app_message(
    db: tauri::State<'_, crate::db::AppDatabase>,
    apps: tauri::State<'_, crate::plugin_runtime::app_host::PluginAppRegistry>,
    router: tauri::State<'_, crate::plugin_runtime::router::PluginRouter>,
    manager: tauri::State<'_, crate::acp::manager::ConnectionManager>,
    request: PluginAppMessageRequest,
) -> Result<PluginAppMessageResponse, AppCommandError> {
    message_core(&db, &apps, &router, &manager, request).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn plugin_app_open(
    db: tauri::State<'_, crate::db::AppDatabase>,
    apps: tauri::State<'_, crate::plugin_runtime::app_host::PluginAppRegistry>,
    router: tauri::State<'_, crate::plugin_runtime::router::PluginRouter>,
    request: PluginAppOpenRequest,
) -> Result<PluginAppOpenResponse, AppCommandError> {
    open_core(&db, &apps, &router, request).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn plugin_app_teardown(
    db: tauri::State<'_, crate::db::AppDatabase>,
    apps: tauri::State<'_, crate::plugin_runtime::app_host::PluginAppRegistry>,
    request: PluginAppTeardownRequest,
) -> Result<(), AppCommandError> {
    teardown_core(&db, &apps, request).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn plugin_app_catalog(
    db: tauri::State<'_, crate::db::AppDatabase>,
    request: CatalogRequest,
) -> Result<Vec<AppChoice>, AppCommandError> {
    catalog_core(&db, request).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn plugin_app_sync(
    db: tauri::State<'_, crate::db::AppDatabase>,
    apps: tauri::State<'_, crate::plugin_runtime::app_host::PluginAppRegistry>,
    request: SyncRequest,
) -> Result<Option<SyncUpdate>, AppCommandError> {
    sync_core(&db, &apps, request).await
}
#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn plugin_app_launch(
    db: tauri::State<'_, crate::db::AppDatabase>,
    apps: tauri::State<'_, crate::plugin_runtime::app_host::PluginAppRegistry>,
    router: tauri::State<'_, crate::plugin_runtime::router::PluginRouter>,
    request: LaunchRequest,
) -> Result<PluginAppOpenResponse, AppCommandError> {
    launch_core(&db, (&apps, &router), request).await
}
