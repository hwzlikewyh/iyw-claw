use serde_json::{json, Value};
use tauri::Manager;

use super::error::BrowserError;
use super::manager::BrowserSessionManager;
use super::types::BrowserIywLoginStatus;

const IYW_COOKIE_DOMAIN: &str = ".iyw.cn";
const IYW_COOKIE_NAME: &str = "iyuanwu_token";
const EXPIRED_COOKIE_TIMESTAMP: f64 = 1.0;

impl BrowserSessionManager {
    pub(crate) async fn refresh_iyw_browser_login(&self) {
        let _guard = self.tab_open_lock.lock().await;
        let tab_id = {
            let state = self.state.read().await;
            if state.runtime.status != super::types::BrowserRuntimeStatus::Running {
                return;
            }
            state.tabs.keys().next().cloned()
        };
        if self.is_native_browser().await {
            if let Some(tab_id) = tab_id {
                self.sync_native_iyw_login(&tab_id).await;
            }
        } else {
            self.sync_iyw_login().await;
        }
    }

    pub(in crate::browser) async fn sync_native_iyw_login(&self, tab_id: &str) {
        let status = match self.iyw_login_cookie().await {
            Ok((cookie, status)) => match self.native.command(tab_id, "set_cookie", cookie).await {
                Ok(_) => status,
                Err(error) => {
                    tracing::warn!(target: "iyw_claw_browser", error_code = ?error.code,
                        "IYW native browser cookie synchronization failed");
                    BrowserIywLoginStatus::Unknown
                }
            },
            Err(()) => BrowserIywLoginStatus::Unknown,
        };
        self.state.write().await.set_iyw_login_status(status);
        tracing::info!(
            engine = "webview",
            outcome = login_outcome(status),
            "IYW browser login synchronization completed"
        );
    }

    pub(super) async fn sync_iyw_login(&self) {
        let status = match self.iyw_login_cookie().await {
            Ok((cookie, status)) => match self.write_iyw_cookie(cookie).await {
                Ok(()) => status,
                Err(_) => BrowserIywLoginStatus::Unknown,
            },
            Err(()) => BrowserIywLoginStatus::Unknown,
        };
        self.state.write().await.set_iyw_login_status(status);
        tracing::info!(target: "iyw_claw_browser", outcome = login_outcome(status),
            "IYW browser login synchronization completed");
    }

    async fn iyw_login_cookie(&self) -> Result<(Value, BrowserIywLoginStatus), ()> {
        let database = self.account_database.read().await.clone();
        // 浏览器注册早于数据库初始化，在首次使用时绑定已有的应用数据库。
        let database = match database {
            Some(database) => database,
            None => {
                let database = self
                    .native
                    .app
                    .get()
                    .and_then(|app| app.try_state::<crate::db::AppDatabase>())
                    .map(|database| database.conn.clone())
                    .ok_or_else(|| {
                        tracing::warn!(target: "iyw_claw_browser",
                        "IYW browser account database is unavailable")
                    })?;
                self.set_database(database.clone()).await;
                database
            }
        };
        match crate::commands::iyw_account::iyw_account_access_token_core(&database).await {
            Ok(Some(token)) if valid_cookie_value(token.expose()) => Ok((
                login_cookie(Some(token.expose())),
                BrowserIywLoginStatus::Authenticated,
            )),
            Ok(None) => Ok((login_cookie(None), BrowserIywLoginStatus::Unauthenticated)),
            Err(error) if error.code == crate::app_error::AppErrorCode::AuthenticationFailed => {
                tracing::info!(target: "iyw_claw_browser", outcome = "session_expired",
                    "IYW browser login cookie is being cleared");
                Ok((login_cookie(None), BrowserIywLoginStatus::Unauthenticated))
            }
            Ok(Some(_)) => {
                tracing::warn!(target: "iyw_claw_browser", outcome = "invalid_token",
                    "IYW browser login was not synchronized");
                Err(())
            }
            Err(error) => {
                tracing::warn!(target: "iyw_claw_browser", outcome = "account_read_failed", error_code = ?error.code,
                    "IYW browser login was not synchronized");
                Err(())
            }
        }
    }

    async fn write_iyw_cookie(&self, cookie: Value) -> Result<(), BrowserError> {
        self.cdp_call("Storage.setCookies", json!({"cookies":[cookie]}), None)
            .await
            .map(|_| ())
    }
}

fn login_cookie(token: Option<&str>) -> Value {
    // 官网通过 document.cookie 读取 token 并设置请求头，需与网站的 Cookie 约定一致。
    let mut cookie = json!({
        "name": IYW_COOKIE_NAME,
        "value": token.unwrap_or_default(),
        "domain": IYW_COOKIE_DOMAIN,
        "path": "/",
        "secure": true,
        "httpOnly": false,
        "sameSite": "Lax"
    });
    if token.is_none() {
        cookie["expires"] = json!(EXPIRED_COOKIE_TIMESTAMP);
    }
    cookie
}

fn valid_cookie_value(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| matches!(byte, 0x21 | 0x23..=0x2b | 0x2d..=0x3a | 0x3c..=0x5b | 0x5d..=0x7e))
}

fn login_outcome(status: BrowserIywLoginStatus) -> &'static str {
    match status {
        BrowserIywLoginStatus::Unknown => "unknown",
        BrowserIywLoginStatus::Authenticated => "authenticated",
        BrowserIywLoginStatus::Unauthenticated => "unauthenticated",
    }
}
