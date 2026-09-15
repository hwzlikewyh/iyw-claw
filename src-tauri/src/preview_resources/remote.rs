use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use axum::{
    body::Body,
    extract::{Extension, Path, Request},
    response::Response,
    routing::get,
    Router,
};
use futures_util::StreamExt;
use tokio_util::sync::CancellationToken;
use tower_http::cors::{Any, CorsLayer};

use super::PreviewResource;
use crate::{
    app_error::AppCommandError,
    db::{service::remote_workspace_connection_service, AppDatabase},
};

struct RemoteResource {
    local_id: String,
    remote_id: String,
    base: String,
    token: String,
    client: reqwest::Client,
    cancelled: CancellationToken,
    touched: Mutex<Instant>,
}

static REMOTE: LazyLock<Mutex<HashMap<String, Arc<RemoteResource>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[tauri::command]
pub async fn open_remote_preview_resource(
    db: tauri::State<'_, AppDatabase>,
    connection_id: i32,
    root_path: String,
    path: String,
) -> Result<PreviewResource, AppCommandError> {
    let conn = remote_workspace_connection_service::get(&db.conn, connection_id)
        .await
        .map_err(AppCommandError::db)?
        .ok_or_else(|| AppCommandError::not_found("Remote connection not found"))?;
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(network_error)?;
    let base = conn.base_url.trim_end_matches('/').to_string();
    let response = client
        .post(format!("{base}/api/open_preview_resource"))
        .bearer_auth(conn.token.trim())
        .json(&serde_json::json!({ "rootPath": root_path, "path": path }))
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(network_error)?;
    let remote: PreviewResource = response
        .error_for_status()
        .map_err(network_error)?
        .json()
        .await
        .map_err(network_error)?;
    if uuid::Uuid::parse_str(&remote.id).is_err() {
        return Err(AppCommandError::invalid_input(
            "Invalid preview resource response",
        ));
    }
    let id = uuid::Uuid::new_v4().simple().to_string();
    let resource = Arc::new(RemoteResource {
        local_id: id.clone(),
        remote_id: remote.id,
        base,
        token: conn.token,
        client,
        cancelled: CancellationToken::new(),
        touched: Mutex::new(Instant::now()),
    });
    let registered = {
        let mut entries = REMOTE.lock().unwrap_or_else(|error| error.into_inner());
        if entries.len() >= super::MAX_PREVIEWS {
            false
        } else {
            entries.insert(id.clone(), resource.clone());
            true
        }
    };
    if !registered {
        release_remote(&resource).await;
        return Err(AppCommandError::invalid_input("Too many open previews"));
    }
    let url = match listen(resource.clone()).await {
        Ok(url) => url,
        Err(error) => {
            close_remote_preview_resource(id).await;
            return Err(error);
        }
    };
    expire(resource);
    Ok(PreviewResource {
        id,
        url,
        size: remote.size,
    })
}

fn network_error(error: reqwest::Error) -> AppCommandError {
    AppCommandError::invalid_input(format!(
        "Preview connection failed: {}",
        error.without_url()
    ))
}

async fn listen(resource: Arc<RemoteResource>) -> Result<String, AppCommandError> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(AppCommandError::io)?;
    let port = listener.local_addr().map_err(AppCommandError::io)?.port();
    let router = Router::new()
        .route("/api/preview_resource/{id}/{*name}", get(proxy))
        .layer(Extension(resource.clone()))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_headers(Any)
                .allow_methods([axum::http::Method::GET])
                .expose_headers(Any),
        );
    let cancel = resource.cancelled.clone();
    tokio::spawn(async move {
        if axum::serve(listener, router)
            .with_graceful_shutdown(cancel.cancelled_owned())
            .await
            .is_err()
        {
            tracing::warn!("[preview] remote byte proxy stopped");
        }
    });
    Ok(format!(
        "http://127.0.0.1:{port}/api/preview_resource/{}/file",
        resource.local_id
    ))
}

async fn proxy(
    Extension(resource): Extension<Arc<RemoteResource>>,
    Path((id, name)): Path<(String, String)>,
    request: Request,
) -> Result<Response, AppCommandError> {
    if id != resource.local_id || resource.cancelled.is_cancelled() {
        return Err(AppCommandError::not_found("Preview closed"));
    }
    let url = format!(
        "{}/api/preview_resource/{}/{}",
        resource.base,
        resource.remote_id,
        urlencoding::encode(&name)
    );
    let mut outgoing = resource.client.get(url);
    for name in ["range", "if-range", "if-modified-since"] {
        if let Some(value) = request.headers().get(name) {
            outgoing = outgoing.header(name, value);
        }
    }
    let incoming = tokio::select! {
        _ = resource.cancelled.cancelled() => return Err(AppCommandError::not_found("Preview closed")),
        response = outgoing.send() => response.map_err(network_error)?,
    };
    let mut response = Response::builder().status(incoming.status());
    for name in [
        "content-type",
        "content-length",
        "content-range",
        "accept-ranges",
        "etag",
        "last-modified",
    ] {
        if let Some(value) = incoming.headers().get(name) {
            response = response.header(name, value);
        }
    }
    let body = incoming
        .bytes_stream()
        .take_until(resource.cancelled.clone().cancelled_owned());
    response
        .header("cache-control", "no-store")
        .header("referrer-policy", "no-referrer")
        .body(Body::from_stream(body))
        .map_err(|_| AppCommandError::invalid_input("Invalid preview response"))
}

#[tauri::command]
pub async fn renew_remote_preview_resource(id: String) -> Result<(), AppCommandError> {
    let resource = REMOTE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .get(&id)
        .cloned()
        .ok_or_else(|| AppCommandError::not_found("Preview closed"))?;
    *resource
        .touched
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Instant::now();
    resource
        .client
        .post(format!("{}/api/renew_preview_resource", resource.base))
        .bearer_auth(resource.token.trim())
        .json(&serde_json::json!({ "id": resource.remote_id }))
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(network_error)?
        .error_for_status()
        .map_err(network_error)?;
    Ok(())
}

#[tauri::command]
pub async fn close_remote_preview_resource(id: String) {
    let resource = REMOTE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .remove(&id);
    if let Some(resource) = resource {
        resource.cancelled.cancel();
        release_remote(&resource).await;
    }
}

async fn release_remote(resource: &RemoteResource) {
    let response = resource
        .client
        .post(format!("{}/api/close_preview_resource", resource.base))
        .bearer_auth(resource.token.trim())
        .json(&serde_json::json!({ "id": resource.remote_id }))
        .timeout(Duration::from_secs(10))
        .send()
        .await;
    if !response.is_ok_and(|response| response.status().is_success()) {
        tracing::warn!("[preview] remote release failed; server lease will expire");
    }
}

fn expire(resource: Arc<RemoteResource>) {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = resource.cancelled.cancelled() => break,
                _ = tokio::time::sleep(super::LEASE_CHECK_INTERVAL) => {
                    let elapsed = resource.touched.lock().unwrap_or_else(|error| error.into_inner()).elapsed();
                    if elapsed >= super::LEASE_TIMEOUT { close_remote_preview_resource(resource.local_id.clone()).await; break; }
                }
            }
        }
    });
}
