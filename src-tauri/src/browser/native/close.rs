use serde_json::{json, Value};

use super::super::error::{BrowserError, BrowserErrorCode};
use super::request::NativeRequest;

impl super::NativeBrowser {
    pub(super) async fn close_tab(&self, id: &str) -> Result<Value, BrowserError> {
        tracing::info!(browser_tab_id = id, "native browser tab close requested");
        let cookies = self
            .request(NativeRequest::new(id, "export_cookies", json!({})))
            .await;
        let cookies = match cookies {
            Ok(value) => value,
            Err(error) => {
                if error.code != BrowserErrorCode::BrowserTabGone {
                    tracing::warn!(browser_tab_id = id, error_code = ?error.code, error = %error,
                        "native cookie export unavailable during tab close");
                }
                json!({})
            }
        };
        // Cookie 回调只返回数据；析构另行排入主线程，不能在 WebView2 回调内 Close。
        let result = self
            .request(NativeRequest::new(id, "close", json!({})))
            .await;
        if let Err(error) = &result {
            tracing::error!(browser_tab_id = id, error_code = ?error.code, error = %error,
                "native browser tab close failed");
        }
        result.map(|_| cookies)
    }
}
