use std::rc::Rc;

use serde_json::Value;
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, WebviewWindow};
use tauri_runtime_wry::wry::{NewWindowOpener, PageLoadEvent, Rect, WebContext, WebViewBuilder};

use super::state::{self, NativePage, CONTEXT, PAGES};

const INITIAL_WIDTH: f64 = 1024.0;
const INITIAL_HEIGHT: f64 = 768.0;

pub(super) fn create(app: &AppHandle, id: &str, values: &Value) -> Result<Value, String> {
    if let Ok(value) = state::snapshot(id) {
        return Ok(value);
    }
    let owner = app
        .get_webview_window("main")
        .ok_or("Browser workspace window is unavailable")?;
    let url = super::super::tab_actions::validated_url(
        values["url"].as_str().ok_or("Browser URL is required")?,
    )
    .map_err(|error| error.message)?;
    let view = build(&owner, id, None)?;
    if let Err(error) = super::download_control::install(&view, app, id) {
        drop(view);
        #[cfg(target_os = "linux")]
        super::linux::remove(id);
        return Err(error);
    }
    PAGES.with(|pages| {
        pages.borrow_mut().insert(
            id.to_string(),
            NativePage::new(owner.label().to_string(), Rc::new(view), url.clone()),
        )
    });
    tracing::info!(
        browser_tab_id = id,
        engine = "webview",
        "native browser tab created"
    );
    state::snapshot(id)
}

pub(super) fn build(
    owner: &WebviewWindow,
    id: &str,
    opener: Option<NewWindowOpener>,
) -> Result<tauri_runtime_wry::wry::WebView, String> {
    #[cfg(target_os = "windows")]
    crate::native_page::window_hook::retain(owner)?;
    let label = format!("iyw-native-{id}");
    CONTEXT.with(|context| {
        let mut context = context.borrow_mut();
        if context.is_none() {
            let directory = super::profile::directory();
            std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
            *context = Some(WebContext::new(Some(directory)));
        }
        let builder = if opener.is_some() {
            WebViewBuilder::new()
        } else {
            WebViewBuilder::new_with_web_context(
                context.as_mut().ok_or("Browser context is unavailable")?,
            )
        };
        let builder = builder
            .with_id(&label)
            .with_bounds(Rect {
                position: LogicalPosition::new(0.0, 0.0).into(),
                size: LogicalSize::new(INITIAL_WIDTH, INITIAL_HEIGHT).into(),
            })
            .with_visible(false)
            .with_devtools(true)
            .with_clipboard(true)
            .with_initialization_script(include_str!("../../native_page/page.js"))
            .with_initialization_script(include_str!("page.js"));
        let builder = handlers(builder, owner, id);
        let builder = super::downloads::attach(builder, owner.app_handle(), id);
        let builder = configure_opener(builder, opener)?;
        #[cfg(not(target_os = "linux"))]
        {
            builder
                .build_as_child(owner)
                .map_err(|error| error.to_string())
        }
        #[cfg(target_os = "linux")]
        {
            super::linux::build(owner.app_handle(), id, builder)
        }
    })
}

fn handlers<'a>(
    builder: WebViewBuilder<'a>,
    owner: &WebviewWindow,
    id: &str,
) -> WebViewBuilder<'a> {
    let app = owner.app_handle().clone();
    let nav_id = id.to_string();
    let nav_app = app.clone();
    let title_id = id.to_string();
    let title_app = app.clone();
    let popup_id = id.to_string();
    let popup_app = app.clone();
    let message_id = id.to_string();
    builder
        .with_navigation_handler(|url| super::super::tab_actions::validated_url(&url).is_ok())
        .with_on_page_load_handler(move |event, url| {
            let _ = state::with_page(&nav_id, |page| {
                page.url = url;
                page.loading = matches!(event, PageLoadEvent::Started);
                if page.loading {
                    page.epoch = page.epoch.saturating_add(1);
                    page.errors = 0;
                }
                Ok(())
            });
            super::metadata::publish(&nav_app, &nav_id);
        })
        .with_document_title_changed_handler(move |title| {
            let _ = state::with_page(&title_id, |page| {
                page.title = title;
                Ok(())
            });
            super::metadata::publish(&title_app, &title_id);
        })
        .with_new_window_req_handler(move |url, features| {
            super::popups::open(&popup_app, &popup_id, url, features)
        })
        .with_ipc_handler(move |request| {
            if request.body().len() > 4096 {
                return;
            }
            if let Ok(value) = serde_json::from_str::<Value>(request.body()) {
                super::metadata::message(&app, &message_id, value);
            }
        })
}

fn configure_opener(
    builder: WebViewBuilder<'_>,
    opener: Option<NewWindowOpener>,
) -> Result<WebViewBuilder<'_>, String> {
    #[cfg(target_os = "windows")]
    {
        use tauri_runtime_wry::wry::WebViewBuilderExtWindows;
        Ok(match opener {
            Some(opener) => builder.with_environment(opener.environment),
            None => builder,
        })
    }
    #[cfg(target_os = "macos")]
    {
        use tauri_runtime_wry::wry::WebViewBuilderExtMacos;
        match opener {
            Some(opener) => Ok(builder.with_webview_configuration(opener.target_configuration)),
            None => super::profile::configure(builder),
        }
    }
    #[cfg(target_os = "linux")]
    {
        use tauri_runtime_wry::wry::WebViewBuilderExtUnix;
        Ok(match opener {
            Some(opener) => builder.with_related_view(opener.webview),
            None => builder,
        })
    }
}
