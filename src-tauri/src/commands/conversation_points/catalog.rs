use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use sea_orm::DatabaseConnection;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use super::pricing::{parse_prices, Prices};
use crate::app_error::AppCommandError;

const PRICE_CACHE_TTL: Duration = Duration::from_secs(300);
const PRICE_FAILURE_TTL: Duration = Duration::from_secs(2);
const PRICE_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

struct CachedPrices {
    scope: Vec<u8>,
    expires: Instant,
    prices: Option<Arc<Prices>>,
}

fn cache() -> &'static Mutex<Option<CachedPrices>> {
    static CACHE: OnceLock<Mutex<Option<CachedPrices>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

pub(super) async fn load(conn: &DatabaseConnection) -> Option<Arc<Prices>> {
    let token = super::super::iyw_account::iyw_account_access_token_core(conn)
        .await
        .ok()??;
    let url = crate::acp::provider_overlay::model_gateway_models_url();
    let mut digest = Sha256::new();
    digest.update(url.as_bytes());
    digest.update(token.expose().as_bytes());
    let scope = digest.finalize().to_vec();
    let mut cached = cache().lock().await;
    if let Some(entry) = cached
        .as_ref()
        .filter(|entry| entry.scope == scope && entry.expires > Instant::now())
    {
        return entry.prices.clone();
    }
    let prices = match fetch(&url, token.expose()).await {
        Ok(prices) => {
            tracing::debug!(
                model_count = prices.len(),
                "[usage-points] prices refreshed"
            );
            Some(Arc::new(prices))
        }
        Err(error) => {
            tracing::warn!(code = ?error.code, message = %error.message, detail = ?error.detail, "[usage-points] prices unavailable; conversation remains usable");
            None
        }
    };
    let ttl = if prices.is_some() {
        PRICE_CACHE_TTL
    } else {
        PRICE_FAILURE_TTL
    };
    *cached = Some(CachedPrices {
        scope,
        prices: prices.clone(),
        expires: Instant::now() + ttl,
    });
    prices
}

async fn fetch(url: &str, token: &str) -> Result<Prices, AppCommandError> {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    let response = CLIENT
        .get_or_init(reqwest::Client::new)
        .get(url)
        .header("token", token)
        .timeout(PRICE_REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|_| AppCommandError::network("Model pricing request failed"))?;
    if !response.status().is_success() {
        return Err(AppCommandError::network("Model pricing request rejected")
            .with_detail(response.status().to_string()));
    }
    let payload = response
        .json::<serde_json::Value>()
        .await
        .map_err(|_| AppCommandError::network("Model pricing response was invalid"))?;
    parse_prices(&payload).ok_or_else(|| AppCommandError::network("Model pricing was absent"))
}
