use std::collections::HashSet;

use crate::models::{DbConversationDetail, MessageTurn, TurnUsage};

mod catalog;
mod pricing;

fn counts(usage: &TurnUsage) -> [u64; 4] {
    [
        usage.input_tokens,
        usage.output_tokens,
        usage.cache_read_input_tokens,
        usage.cache_creation_input_tokens,
    ]
}

fn unique_model<'a>(turns: &'a [MessageTurn], fallback: Option<&'a str>) -> Option<&'a str> {
    let models: HashSet<_> = turns
        .iter()
        .filter_map(|turn| turn.model.as_deref())
        .filter(|model| !model.trim().is_empty())
        .collect();
    match models.len() {
        0 => fallback,
        1 => models.into_iter().next(),
        _ => None,
    }
}

pub(super) async fn enrich(conn: &sea_orm::DatabaseConnection, detail: &mut DbConversationDetail) {
    if detail
        .session_stats
        .as_ref()
        .and_then(|stats| stats.total_usage.as_ref())
        .is_none()
        && !detail.turns.iter().any(|turn| turn.usage.is_some())
    {
        return;
    }
    let Some(prices) = catalog::load(conn).await else {
        return;
    };
    apply(&prices, detail);
}

fn annotate_turns(
    prices: &pricing::Prices,
    turns: &mut [MessageTurn],
    single_model: Option<&str>,
) -> ([u64; 4], Option<f64>, bool) {
    let mut total_counts = [0u64; 4];
    let mut total_points = Some(0.0);
    let mut has_usage = false;
    for turn in turns {
        let Some(usage) = turn.usage.as_mut() else {
            continue;
        };
        has_usage = true;
        let model = turn
            .model
            .as_deref()
            .filter(|model| !model.trim().is_empty())
            .or(single_model);
        usage.estimated_points = if counts(usage) == [0; 4] {
            Some(0.0)
        } else {
            model
                .and_then(|model| pricing::find_price(prices, model))
                .and_then(|price| price.estimate(usage))
        };
        total_points = total_points
            .zip(usage.estimated_points)
            .map(|(sum, value)| sum + value)
            .filter(|sum| sum.is_finite());
        for (total, value) in total_counts.iter_mut().zip(counts(usage)) {
            *total = total.saturating_add(value);
        }
    }
    (total_counts, total_points, has_usage)
}

fn apply(prices: &pricing::Prices, detail: &mut DbConversationDetail) {
    let single_model =
        unique_model(&detail.turns, detail.summary.model.as_deref()).map(str::to_owned);
    let (total_counts, total_points, has_usage) =
        annotate_turns(prices, &mut detail.turns, single_model.as_deref());
    let Some(total) = detail
        .session_stats
        .as_mut()
        .and_then(|stats| stats.total_usage.as_mut())
    else {
        return;
    };
    // 多模型会话按各轮费用相加；单模型可直接使用完整累计量，避免分页和日志缺项少算。
    total.estimated_points = if has_usage && total_counts == counts(total) {
        total_points
    } else {
        single_model
            .as_deref()
            .and_then(|model| pricing::find_price(prices, model))
            .and_then(|price| price.estimate(total))
    };
}
