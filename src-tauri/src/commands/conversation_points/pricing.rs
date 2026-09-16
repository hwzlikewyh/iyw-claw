use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value;

use crate::models::TurnUsage;

// 旧版模型目录未发布换算系数时，沿用已公开的 1 元 = 10 点合同。
const LEGACY_POINTS_PER_CNY: f64 = 10.0;
const TOKENS_PER_MILLION: f64 = 1_000_000.0;

#[derive(Clone, Deserialize, PartialEq)]
pub(super) struct TokenPrice {
    currency: String,
    unit: String,
    input: f64,
    output: f64,
    cached_input: f64,
    cache_creation: f64,
    #[serde(default)]
    points_per_cny: Option<String>,
    #[serde(default)]
    reasoning_output: f64,
    #[serde(default)]
    cache_creation_5m: f64,
    #[serde(default)]
    cache_creation_1h: f64,
    #[serde(default)]
    image_billing_mode: String,
}

impl TokenPrice {
    pub(super) fn estimate(&self, usage: &TurnUsage) -> Option<f64> {
        if self.currency != "CNY"
            || self.unit != "per_1m_tokens"
            || self.image_billing_mode == "per_image"
        {
            return None;
        }
        // 日志没有推理/缓存 TTL 明细时，不能猜测差异化费率的分摊。
        if usage.output_tokens > 0
            && self.reasoning_output > 0.0
            && self.reasoning_output != self.output
        {
            return None;
        }
        if usage.cache_creation_input_tokens > 0
            && (self.cache_creation_5m > 0.0 || self.cache_creation_1h > 0.0)
        {
            return None;
        }
        let rate = match self.points_per_cny.as_deref() {
            Some(value) => value.parse::<f64>().ok()?,
            None => LEGACY_POINTS_PER_CNY,
        };
        let prices = [
            self.input,
            self.output,
            self.cached_input,
            self.cache_creation,
            rate,
        ];
        if prices
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
        {
            return None;
        }
        let cost = usage.input_tokens as f64 * self.input
            + usage.output_tokens as f64 * self.output
            + usage.cache_read_input_tokens as f64 * self.cached_input
            + usage.cache_creation_input_tokens as f64 * self.cache_creation;
        let points = cost / TOKENS_PER_MILLION * rate;
        points.is_finite().then_some(points)
    }
}

pub(super) type Prices = HashMap<String, TokenPrice>;

pub(super) fn parse_prices(payload: &Value) -> Option<Prices> {
    let models = payload.get("data")?.as_array()?;
    let mut result = HashMap::new();
    for model in models {
        let Some(id) = model.get("id").and_then(Value::as_str) else {
            continue;
        };
        let Some(raw) = model.get("prices") else {
            continue;
        };
        let Ok(prices) = serde_json::from_value::<Vec<TokenPrice>>(raw.clone()) else {
            continue;
        };
        let Some(price) = prices.first() else {
            continue;
        };
        if prices.iter().all(|candidate| candidate == price) {
            result.insert(id.to_string(), price.clone());
        }
    }
    Some(result)
}

pub(super) fn find_price<'a>(prices: &'a Prices, model: &str) -> Option<&'a TokenPrice> {
    prices
        .get(model)
        .or_else(|| prices.get(model.strip_prefix("iyw-claw/")?))
}
