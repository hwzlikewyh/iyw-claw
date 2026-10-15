use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use serde_json::{json, Value};
use tauri_runtime_wry::wry::{WebContext, WebView};

pub(super) struct NativePage {
    pub owner: String,
    pub view: Rc<WebView>,
    pub url: String,
    pub title: String,
    pub loading: bool,
    pub epoch: u64,
    pub errors: u32,
    pub visible: bool,
    pub find_term: String,
    pub agent_input_until: Option<std::time::Instant>,
}

thread_local! {
    pub(super) static PAGES: RefCell<HashMap<String, NativePage>> = RefCell::new(HashMap::new());
    pub(super) static CONTEXT: RefCell<Option<WebContext>> = const { RefCell::new(None) };
}

impl NativePage {
    pub(super) fn new(owner: String, view: Rc<WebView>, url: String) -> Self {
        Self {
            owner,
            view,
            url,
            title: String::new(),
            loading: true,
            epoch: 1,
            errors: 0,
            visible: false,
            find_term: String::new(),
            agent_input_until: None,
        }
    }
}

pub(super) fn with_page<T>(
    id: &str,
    operation: impl FnOnce(&mut NativePage) -> Result<T, String>,
) -> Result<T, String> {
    PAGES.with(|pages| {
        operation(
            pages
                .borrow_mut()
                .get_mut(id)
                .ok_or("Native browser tab is closed")?,
        )
    })
}

pub(super) fn view(id: &str) -> Result<Rc<WebView>, String> {
    with_page(id, |page| Ok(page.view.clone()))
}

pub(super) fn snapshot(id: &str) -> Result<Value, String> {
    with_page(id, |page| {
        let url = current_url(page).unwrap_or_else(|| page.url.clone());
        Ok(
            json!({"url": url, "title": page.title, "loading": page.loading, "epoch": page.epoch, "errors": page.errors}),
        )
    })
}

#[cfg(target_os = "macos")]
fn current_url(page: &NativePage) -> Option<String> {
    use tauri_runtime_wry::wry::WebViewExtMacOS;
    unsafe {
        page.view
            .webview()
            .URL()
            .and_then(|value| value.absoluteString())
            .map(|value| value.to_string())
    }
}

#[cfg(not(target_os = "macos"))]
fn current_url(page: &NativePage) -> Option<String> {
    page.view.url().ok().filter(|value| !value.is_empty())
}

pub(super) fn remove(id: &str) -> Result<(), String> {
    let page = PAGES.with(|pages| pages.borrow_mut().remove(id));
    if let Some(page) = page {
        destroy(id, page);
    }
    #[cfg(target_os = "linux")]
    super::linux::remove(id);
    Ok(())
}

pub(super) fn clear() {
    let pages = PAGES.with(|pages| std::mem::take(&mut *pages.borrow_mut()));
    tracing::info!(
        page_count = pages.len(),
        "native browser pages cleanup started"
    );
    for (id, page) in pages {
        destroy(&id, page);
        #[cfg(target_os = "linux")]
        super::linux::remove(&id);
    }
    let context = CONTEXT.with(|context| context.borrow_mut().take());
    drop(context);
    tracing::info!("native browser pages cleanup completed");
}

fn destroy(id: &str, page: NativePage) {
    tracing::info!(
        browser_tab_id = id,
        view_references = Rc::strong_count(&page.view),
        "native browser tab destruction started"
    );
    if let Err(error) = page.view.set_visible(false) {
        tracing::warn!(browser_tab_id = id, %error, "native browser tab hide failed during close");
    }
    if let Err(error) = crate::native_page::platform::navigate(&page.view, "stop") {
        tracing::warn!(browser_tab_id = id, %error, "native browser tab stop failed during close");
    }
    // 先从状态表移除并结束借用，再让 Wry 在主线程执行 controller.Close。
    drop(page);
    tracing::info!(
        browser_tab_id = id,
        "native browser tab destruction completed"
    );
}
