use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Method, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::Router;
use tower_http::services::{ServeDir, ServeFile};

type EmbeddedAssetLoader = Arc<dyn Fn(String) -> Option<EmbeddedAsset> + Send + Sync + 'static>;

#[derive(Clone)]
pub struct StaticAssetSource {
    kind: StaticAssetSourceKind,
}

#[derive(Clone)]
enum StaticAssetSourceKind {
    Directory(PathBuf),
    Embedded(EmbeddedAssetLoader),
}

struct EmbeddedAsset {
    bytes: Vec<u8>,
    mime_type: String,
    csp_header: Option<String>,
}

impl StaticAssetSource {
    pub fn directory(path: PathBuf) -> Self {
        Self {
            kind: StaticAssetSourceKind::Directory(path),
        }
    }

    #[cfg(feature = "tauri-runtime")]
    pub fn tauri(app: tauri::AppHandle) -> Self {
        use tauri::Manager;

        let loader: EmbeddedAssetLoader = Arc::new(move |path| {
            app.asset_resolver().get(path).map(|asset| EmbeddedAsset {
                bytes: asset.bytes,
                mime_type: asset.mime_type,
                csp_header: asset.csp_header,
            })
        });
        Self {
            kind: StaticAssetSourceKind::Embedded(loader),
        }
    }
}

pub fn mount(router: Router, source: StaticAssetSource) -> Router {
    match source.kind {
        StaticAssetSourceKind::Directory(path) => mount_directory(router, path),
        StaticAssetSourceKind::Embedded(loader) => mount_embedded(router, loader),
    }
}

fn mount_directory(router: Router, path: PathBuf) -> Router {
    let fallback = ServeDir::new(&path).fallback(ServeFile::new(path.join("index.html")));
    let rewrite_root = path.clone();
    let html_rewrite = middleware::from_fn(move |req: axum::extract::Request, next: Next| {
        let root = rewrite_root.clone();
        async move { rewrite_directory_html(req, next, root).await }
    });

    router.fallback_service(fallback).layer(html_rewrite)
}

async fn rewrite_directory_html(
    req: axum::extract::Request,
    next: Next,
    root: PathBuf,
) -> Response {
    let path = req.uri().path();
    if path == "/" || path.contains('.') || path.starts_with("/api") || path.starts_with("/ws") {
        return next.run(req).await;
    }

    let html_path = format!("{}.html", path.trim_end_matches('/'));
    if !root.join(html_path.trim_start_matches('/')).exists() {
        return next.run(req).await;
    }

    let rewritten = match req.uri().query() {
        Some(query) => format!("{html_path}?{query}"),
        None => html_path,
    };
    let Ok(uri) = rewritten.parse::<Uri>() else {
        return next.run(req).await;
    };
    let (mut parts, body) = req.into_parts();
    parts.uri = uri;
    next.run(axum::extract::Request::from_parts(parts, body))
        .await
}

fn mount_embedded(router: Router, loader: EmbeddedAssetLoader) -> Router {
    tracing::info!("[WEB] Serving static files from Tauri embedded assets");
    router.fallback(move |req: axum::extract::Request| {
        let loader = Arc::clone(&loader);
        async move { serve_embedded_asset(loader, req) }
    })
}

fn serve_embedded_asset(loader: EmbeddedAssetLoader, req: axum::extract::Request) -> Response {
    let method = req.method().clone();
    let uri = req.uri().clone();
    if method != Method::GET && method != Method::HEAD {
        return (
            StatusCode::METHOD_NOT_ALLOWED,
            [(header::ALLOW, "GET, HEAD")],
        )
            .into_response();
    }

    let Some(asset) = load_embedded_asset(&loader, uri.path()) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let length = asset.bytes.len();
    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, asset.mime_type)
        .header(header::CONTENT_LENGTH, length)
        .header("x-content-type-options", "nosniff");
    if let Some(csp) = asset.csp_header {
        builder = builder.header("content-security-policy", csp);
    }
    let body = if method == Method::HEAD {
        Body::empty()
    } else {
        Body::from(asset.bytes)
    };
    builder.body(body).unwrap_or_else(|error| {
        tracing::error!(
            error = %error,
            path = %uri.path(),
            "[WEB] Failed to build embedded asset response"
        );
        StatusCode::INTERNAL_SERVER_ERROR.into_response()
    })
}

fn load_embedded_asset(loader: &EmbeddedAssetLoader, path: &str) -> Option<EmbeddedAsset> {
    let path = normalize_asset_path(path)?;
    let trimmed = path.trim_end_matches('/');
    let candidates = if trimmed.is_empty() {
        vec!["/index.html".to_string()]
    } else {
        vec![
            path.clone(),
            format!("{trimmed}.html"),
            format!("{trimmed}/index.html"),
            "/index.html".to_string(),
        ]
    };

    candidates
        .into_iter()
        .find_map(|candidate| loader(candidate))
}

fn normalize_asset_path(path: &str) -> Option<String> {
    let decoded = urlencoding::decode(path).ok()?;
    // Tauri dev reads from frontendDist with filesystem joins, so reject
    // components that could escape the asset root on either path style.
    // AssetResolver decodes once more. Reject a residual escape marker so a
    // double-encoded separator or `..` cannot change the validated structure.
    if decoded.contains('%') || decoded.contains('\\') || decoded.chars().any(char::is_control) {
        return None;
    }

    let mut normalized = String::from('/');
    let mut has_component = false;
    for component in decoded.split('/') {
        if component.is_empty() || component == "." {
            continue;
        }
        if component == ".."
            || component.ends_with('.')
            || component.ends_with(' ')
            || component.contains(':')
        {
            return None;
        }
        if has_component {
            normalized.push('/');
        }
        normalized.push_str(component);
        has_component = true;
    }
    Some(normalized)
}
