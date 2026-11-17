use std::collections::HashMap;
use std::convert::Infallible;
use std::path::{Component, PathBuf};
use std::sync::Arc;

use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State as AxumState};
use axum::http::{header, StatusCode};
use axum::response::sse::{Event, KeepAlive};
use axum::response::{Html, IntoResponse, Response, Sse};
use axum::{routing::get, Json, Router};
use futures_util::stream;
use tokio::io::AsyncReadExt;

use super::State;

const PAGE: &str = include_str!("../../resources/office-preview/index.html");
const BOOT: &str = include_str!("../../resources/office-preview/boot.js");
const VIEWER: &str = include_str!("../../resources/office-preview/viewer.js");
const GRID: &str = include_str!("../../resources/office-preview/grid.js");
const GRID_CSS: &str = include_str!("../../resources/office-preview/grid.css");
const LICENSES: &str = include_str!("../../resources/office-preview/licenses.json");

pub(super) fn router(state: Arc<State>) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/status", get(status))
        .route("/events", get(events))
        .route("/licenses", get(licenses))
        .route("/data/{*path}", get(asset))
        .with_state(state)
}

async fn licenses(
    AxumState(state): AxumState<Arc<State>>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    if !authorized(&state, &query) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    (
        [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
        LICENSES,
    )
        .into_response()
}

fn authorized(state: &State, query: &HashMap<String, String>) -> bool {
    !state.cancel.is_cancelled()
        && query
            .get("cap")
            .is_some_and(|cap| super::super::constant_time_eq(cap.as_bytes(), state.cap.as_bytes()))
}

async fn index(
    AxumState(state): AxumState<Arc<State>>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    if !authorized(&state, &query) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let (engine, css, view) = if state.grid {
        (GRID, GRID_CSS, "<div id=\"app\"></div>")
    } else {
        (
            VIEWER,
            "",
            "<iframe id=\"viewer\" title=\"Document\"></iframe>",
        )
    };
    let html = PAGE
        .replace("__GRID_CSS__", css)
        .replace("__VIEW__", view)
        .replace("__ENGINE__", &engine.replace("</script", "<\\/script"))
        .replace("__BOOT__", BOOT);
    (
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::REFERRER_POLICY, "no-referrer"),
        ],
        Html(html),
    )
        .into_response()
}

async fn status(
    AxumState(state): AxumState<Arc<State>>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    if !authorized(&state, &query) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(state.versions.borrow().clone()),
    )
        .into_response()
}

async fn events(
    AxumState(state): AxumState<Arc<State>>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    if !authorized(&state, &query) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let receiver = state.versions.subscribe();
    let cancel = state.cancel.clone();
    let updates = stream::unfold(
        (receiver, true, cancel),
        |(mut receiver, first, cancel)| async move {
            if !first {
                tokio::select! {
                    _ = cancel.cancelled() => return None,
                    changed = receiver.changed() => if changed.is_err() { return None; }
                }
            }
            let generation = receiver.borrow_and_update().generation.clone();
            Some((
                Ok::<_, Infallible>(Event::default().data(generation)),
                (receiver, false, cancel),
            ))
        },
    );
    Sse::new(updates)
        .keep_alive(KeepAlive::default())
        .into_response()
}

async fn asset(
    AxumState(state): AxumState<Arc<State>>,
    Query(query): Query<HashMap<String, String>>,
    Path(path): Path<String>,
) -> Response {
    if !authorized(&state, &query) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let relative = PathBuf::from(&path);
    if relative
        .components()
        .any(|part| !matches!(part, Component::Normal(_)))
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    asset_file(&state, relative).await
}

async fn asset_file(state: &State, relative: PathBuf) -> Response {
    let target = state.root.path().join(&relative);
    let Ok(metadata) = tokio::fs::metadata(&target).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !metadata.is_file() || metadata.len() > 64 * 1024 * 1024 {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Ok(file) = tokio::fs::File::open(target).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let content = stream::try_unfold(file, |mut file| async move {
        let mut buffer = vec![0u8; 64 * 1024];
        let count = file.read(&mut buffer).await?;
        if count == 0 {
            return Ok::<_, std::io::Error>(None);
        }
        buffer.truncate(count);
        Ok(Some((Bytes::from(buffer), file)))
    });
    (
        [
            (header::CONTENT_TYPE, mime(&relative)),
            (header::CACHE_CONTROL, "no-store"),
            (header::REFERRER_POLICY, "no-referrer"),
        ],
        Body::from_stream(content),
    )
        .into_response()
}

fn mime(relative: &std::path::Path) -> &'static str {
    match relative
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
    {
        "json" => "application/json",
        "css" => "text/css",
        "html" => "text/plain; charset=utf-8",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}
