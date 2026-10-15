use serde_json::json;

use super::manager::BrowserSessionManager;
use super::types::BrowserIywLoginStatus;

const IYW_COOKIE_DOMAIN: &str = ".iyw.cn";
const IYW_COOKIE_NAME: &str = "iyuanwu_token";

impl BrowserSessionManager {
    pub(in crate::browser) async fn sync_native_iyw_login(&self, tab_id: &str) {
        let status = match self.iyw_account_token().await {
            Some(token) if valid_cookie_value(token.expose()) => {
                let result = self.native.command(tab_id, "set_cookie", json!({"name":IYW_COOKIE_NAME,"value":token.expose(),"domain":IYW_COOKIE_DOMAIN,"path":"/","secure":true,"httpOnly":true,"sameSite":"Lax"})).await;
                if result.is_ok() {
                    BrowserIywLoginStatus::Authenticated
                } else {
                    BrowserIywLoginStatus::Unauthenticated
                }
            }
            _ => BrowserIywLoginStatus::Unauthenticated,
        };
        self.state.write().await.set_iyw_login_status(status);
        tracing::info!(
            engine = "webview",
            outcome = login_outcome(status),
            "IYW browser login synchronization completed"
        );
    }

    pub(super) async fn sync_iyw_login(&self) {
        let status = match self.iyw_account_token().await {
            Some(token) if valid_cookie_value(token.expose()) => {
                self.write_iyw_cookie(token.expose()).await
            }
            Some(_) => {
                tracing::warn!(target: "iyw_claw_browser", outcome = "invalid_token",
                    "IYW browser login was not synchronized");
                BrowserIywLoginStatus::Unauthenticated
            }
            None => BrowserIywLoginStatus::Unauthenticated,
        };
        self.state.write().await.set_iyw_login_status(status);
        tracing::info!(target: "iyw_claw_browser", outcome = login_outcome(status),
            "IYW browser login synchronization completed");
    }

    async fn iyw_account_token(
        &self,
    ) -> Option<crate::acp::account_credentials::AccountAccessToken> {
        let database = self.account_database.read().await.clone()?;
        match crate::commands::iyw_account::iyw_account_access_token_core(&database).await {
            Ok(token) => token,
            Err(_) => {
                tracing::warn!(target: "iyw_claw_browser", outcome = "account_read_failed",
                    "IYW browser login was not synchronized");
                None
            }
        }
    }

    async fn write_iyw_cookie(&self, token: &str) -> BrowserIywLoginStatus {
        let result = self
            .cdp_call(
                "Storage.setCookies",
                json!({"cookies":[{
                    "name": IYW_COOKIE_NAME,
                    "value": token,
                    "domain": IYW_COOKIE_DOMAIN,
                    "path": "/",
                    "secure": true,
                    "httpOnly": true,
                    "sameSite": "Lax"
                }]}),
                None,
            )
            .await;
        if result.is_ok() {
            BrowserIywLoginStatus::Authenticated
        } else {
            BrowserIywLoginStatus::Unauthenticated
        }
    }
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
