use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use serde_json::{json, Value};
use tauri_runtime_wry::wry::{WebContext, WebView};

pub(super) struct Page {
    pub owner: String,
    pub view: Rc<WebView>,
    pub url: String,
    pub title: String,
    pub loading: bool,
    pub pick: Option<String>,
    pub errors: Vec<Value>,
    pub find_term: String,
}

thread_local! {
    pub(super) static PAGES: RefCell<HashMap<String, Page>> = RefCell::new(HashMap::new());
    pub(super) static CONTEXT: RefCell<Option<WebContext>> = const { RefCell::new(None) };
}

pub(super) const EVENT: &str = "interactive-page://event";
pub(super) const MAX_NATIVE_PAGES: usize = 8;
pub(super) const MAX_PAGE_ERRORS: usize = 100;
pub(super) const MAX_MESSAGE_BYTES: usize = 64 * 1024;
pub(super) const MAX_URL_BYTES: usize = 8192;

pub(super) fn snapshot(id: &str, page: &Page) -> Value {
    let (back, forward) = super::platform::history(&page.view);
    // SPA 的 History API 不触发加载回调，快照直接读取引擎当前地址。
    let url = page
        .view
        .url()
        .ok()
        .and_then(|url| valid_url(&url).ok())
        .unwrap_or_else(|| page.url.clone());
    json!({"id": id, "url": url, "title": page.title,
        "loading": page.loading, "canBack": back, "canForward": forward,
        "errorCount": page.errors.len()})
}

pub(super) fn with_page<T>(
    owner: &str,
    id: &str,
    operation: impl FnOnce(&mut Page) -> Result<T, String>,
) -> Result<T, String> {
    PAGES.with(|pages| {
        let mut pages = pages.borrow_mut();
        let page = pages.get_mut(id).ok_or("Native page is closed")?;
        if page.owner != owner {
            return Err("Native page belongs to another window".into());
        }
        operation(page)
    })
}

pub(super) fn view(owner: &str, id: &str) -> Result<Rc<WebView>, String> {
    with_page(owner, id, |page| Ok(page.view.clone()))
}

pub(super) fn close_owner(owner: &str) {
    // 先取出视图再析构，防止引擎回调重入状态表。
    let removed = PAGES.with(|pages| {
        let mut pages = pages.borrow_mut();
        let ids: Vec<_> = pages
            .iter()
            .filter(|(_, page)| page.owner == owner)
            .map(|(id, _)| id.clone())
            .collect();
        ids.into_iter()
            .filter_map(|id| pages.remove(&id))
            .collect::<Vec<_>>()
    });
    drop(removed);
}

pub(super) fn valid_url(value: &str) -> Result<String, String> {
    if value.len() > MAX_URL_BYTES {
        return Err("URL exceeds the supported size".into());
    }
    let url = reqwest::Url::parse(value).map_err(|_| "Expected an HTTP or HTTPS URL")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Expected an HTTP or HTTPS URL without credentials".into());
    }
    Ok(url.to_string())
}
