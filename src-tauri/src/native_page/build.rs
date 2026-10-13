use serde_json::{json, Value};
use std::rc::Rc;
use tauri::{Emitter, Manager, WebviewWindow};
use tauri_runtime_wry::wry::{
    dpi::{LogicalPosition, LogicalSize},
    NewWindowResponse, PageLoadEvent, Rect, WebContext, WebViewBuilder,
};

use super::state::{self, Page, CONTEXT, EVENT, PAGES};

pub(super) fn create(owner: &WebviewWindow, id: &str, request: &Value) -> Result<Value, String> {
    let url = state::valid_url(request["url"].as_str().ok_or("URL is required")?)?;
    let count = PAGES.with(|pages| {
        pages
            .borrow()
            .values()
            .filter(|page| page.owner == owner.label())
            .count()
    });
    if count >= state::MAX_NATIVE_PAGES {
        return Err("Native page limit reached".into());
    }
    let view = build_view(owner, id, request)?;
    let view = Rc::new(view);
    let page = Page {
        owner: owner.label().to_string(),
        view: view.clone(),
        url: url.clone(),
        title: String::new(),
        loading: true,
        pick: None,
        errors: Vec::new(),
        find_term: String::new(),
    };
    PAGES.with(|pages| pages.borrow_mut().insert(id.to_string(), page));
    if let Err(error) = view.load_url(&url) {
        let removed = PAGES.with(|pages| pages.borrow_mut().remove(id));
        drop(removed);
        return Err(error.to_string());
    }
    tracing::info!(page_id = id, owner = owner.label(), "native page created");
    state::with_page(owner.label(), id, |page| Ok(state::snapshot(id, page)))
}

pub(super) fn bounds(value: &Value) -> Result<Rect, String> {
    let number = |key: &str| {
        value[key]
            .as_f64()
            .filter(|value| value.is_finite())
            .ok_or_else(|| format!("Invalid WebView bounds: {key}"))
    };
    let width = number("width")?;
    let height = number("height")?;
    if width <= 0.0 || height <= 0.0 {
        return Err("WebView dimensions must be positive".into());
    }
    Ok(Rect {
        position: LogicalPosition::new(number("x")?, number("y")?).into(),
        size: LogicalSize::new(width, height).into(),
    })
}

fn attach_handlers<'a>(
    builder: WebViewBuilder<'a>,
    owner: &WebviewWindow,
    id: &str,
) -> WebViewBuilder<'a> {
    let load_owner = owner.clone();
    let load_id = id.to_string();
    let title_owner = owner.clone();
    let title_id = id.to_string();
    let ipc_owner = owner.clone();
    let ipc_id = id.to_string();
    let popup_owner = owner.clone();
    let popup_id = id.to_string();
    builder
        .with_navigation_handler(|url| state::valid_url(&url).is_ok())
        .with_on_page_load_handler(move |event, url| {
            tracing::debug!(
                page_id = load_id,
                owner = load_owner.label(),
                started = matches!(event, PageLoadEvent::Started),
                "native page load changed"
            );
            update(&load_owner, &load_id, |page| {
                page.url = url;
                page.loading = matches!(event, PageLoadEvent::Started);
                if page.loading {
                    page.pick = None;
                    page.errors.clear();
                    page.find_term.clear();
                }
            });
        })
        .with_document_title_changed_handler(move |title| {
            update(&title_owner, &title_id, |page| page.title = title)
        })
        .with_ipc_handler(move |request| receive(&ipc_owner, &ipc_id, request.body()))
        .with_new_window_req_handler(move |url, _| {
            if let Ok(url) = state::valid_url(&url) {
                let _ = popup_owner.emit(
                    EVENT,
                    json!({"id": popup_id, "type": "new-tab", "url": url}),
                );
            }
            NewWindowResponse::Deny
        })
}

fn update(owner: &WebviewWindow, id: &str, operation: impl FnOnce(&mut Page)) {
    let result = state::with_page(owner.label(), id, |page| {
        operation(page);
        Ok(state::snapshot(id, page))
    });
    if let Ok(state) = result {
        let _ = owner.emit(EVENT, json!({"id": id, "type": "state", "state": state}));
    }
}

fn receive(owner: &WebviewWindow, id: &str, body: &str) {
    if body.len() > state::MAX_MESSAGE_BYTES {
        return;
    }
    let Ok(mut value) = serde_json::from_str::<Value>(body) else {
        return;
    };
    if !value.is_object() {
        return;
    }
    let accepted = state::with_page(owner.label(), id, |page| {
        match value["type"].as_str() {
            Some("error") => {
                if page.errors.len() < state::MAX_PAGE_ERRORS {
                    page.errors.push(value.clone());
                } else {
                    return Ok(false);
                }
            }
            Some("picked")
                if page.pick.as_deref() == value["token"].as_str() && page.pick.is_some() =>
            {
                page.pick = None;
            }
            Some("shortcut") if value["key"] == "find" => {}
            _ => return Ok(false),
        }
        Ok(true)
    });
    if accepted == Ok(true) {
        value["id"] = json!(id);
        let _ = owner.emit(EVENT, value);
    }
}

fn build_view(
    owner: &WebviewWindow,
    id: &str,
    request: &Value,
) -> Result<tauri_runtime_wry::wry::WebView, String> {
    #[cfg(target_os = "windows")]
    super::window_hook::retain(owner)?;
    CONTEXT.with(|context| {
        let mut context = context.borrow_mut();
        if context.is_none() {
            let directory = owner
                .app_handle()
                .path()
                .app_data_dir()
                .map_err(|error| error.to_string())?
                .join("interactive-webview");
            std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
            *context = Some(WebContext::new(Some(directory)));
        }
        let builder = WebViewBuilder::new_with_web_context(context.as_mut().expect("initialized"))
            .with_devtools(true)
            .with_clipboard(true)
            .with_visible(false)
            .with_bounds(bounds(&request["bounds"])?)
            .with_initialization_script(include_str!("page.js"));
        let builder = super::downloads::attach(attach_handlers(builder, owner, id), owner, id);
        #[cfg(target_os = "macos")]
        let builder = macos_configuration(builder)?;
        builder
            .build_as_child(owner)
            .map_err(|error| error.to_string())
    })
}

#[cfg(target_os = "macos")]
fn macos_configuration(builder: WebViewBuilder<'_>) -> Result<WebViewBuilder<'_>, String> {
    use tauri_runtime_wry::wry::WebViewBuilderExtMacos;
    let mtm = objc2::MainThreadMarker::new().ok_or("WebView must be created on the main thread")?;
    // SAFETY: 已确认当前为主线程，配置对象由 WebKit 初始化并交由 WebView 持有。
    let config = unsafe { objc2_web_kit::WKWebViewConfiguration::new(mtm) };
    unsafe {
        config.setWebsiteDataStore(&objc2_web_kit::WKWebsiteDataStore::nonPersistentDataStore(
            mtm,
        ));
    }
    Ok(builder.with_webview_configuration(config))
}
