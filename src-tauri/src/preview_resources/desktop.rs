use axum::routing::get;
use axum::Router;
use tower_http::cors::{Any, CorsLayer};

use super::{OpenPreviewParams, PreviewResource};
use crate::app_error::AppCommandError;

#[tauri::command]
pub async fn open_preview_resource(
    root_path: String,
    path: String,
) -> Result<PreviewResource, AppCommandError> {
    let resource = super::open(OpenPreviewParams { root_path, path }).await?;
    match start_listener(&resource.id).await {
        Ok(base) => Ok(PreviewResource {
            url: format!("{base}{}", resource.url),
            ..resource
        }),
        Err(error) => {
            super::close(&resource.id);
            Err(error)
        }
    }
}

async fn start_listener(id: &str) -> Result<String, AppCommandError> {
    let resource = super::resources()
        .get(id)
        .cloned()
        .ok_or_else(|| AppCommandError::not_found("Preview closed"))?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(AppCommandError::io)?;
    let port = listener.local_addr().map_err(AppCommandError::io)?.port();
    let router = Router::new()
        .route("/api/preview_resource/{id}/{*name}", get(super::serve))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_headers(Any)
                .allow_methods([axum::http::Method::GET])
                .expose_headers(Any),
        );
    tokio::spawn(async move {
        let result = axum::serve(listener, router)
            .with_graceful_shutdown(resource.cancelled.clone().cancelled_owned())
            .await;
        if let Err(error) = result {
            tracing::warn!(%error, "[preview] local resource server stopped");
        }
    });
    Ok(format!("http://127.0.0.1:{port}"))
}

#[tauri::command]
pub async fn close_preview_resource(id: String) {
    super::close(&id);
}

#[tauri::command]
pub async fn renew_preview_resource(id: String) -> Result<(), AppCommandError> {
    super::renew(&id)
}
