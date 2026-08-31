//! Model-specific context and output budgets shared by provider overlays and
//! runtime diagnostics.

use super::model_catalog::{model_capabilities, ModelLimits};

const USABLE_CONTEXT_PERCENT: u64 = 95;
const DEFAULT_AUTO_COMPACT_PERCENT: u64 = 90;
const ONE_MILLION_COMPACTION_THRESHOLD: u64 = 358_000;
const TWO_HUNDRED_THOUSAND_COMPACTION_THRESHOLD: u64 = 120_000;

pub fn limits_for(model: Option<&str>, reported_context_window: u64) -> ModelLimits {
    let limits = model
        .and_then(model_capabilities)
        .map(|snapshot| snapshot.limits)
        .unwrap_or_default();
    ModelLimits {
        context_window: positive_option(limits.context_window)
            .or_else(|| positive_value(reported_context_window))
            .or_else(|| {
                model.and_then(|model| crate::parsers::infer_context_window_max_tokens(Some(model)))
            }),
        max_input_tokens: positive_option(limits.max_input_tokens),
        max_output_tokens: positive_option(limits.max_output_tokens),
        compaction_at_tokens: positive_option(limits.compaction_at_tokens),
    }
}

pub fn context_window(model: Option<&str>, reported_context_window: u64) -> Option<u64> {
    limits_for(model, reported_context_window).context_window
}

pub fn max_output_tokens(model: Option<&str>, reported_context_window: u64) -> Option<u64> {
    limits_for(model, reported_context_window).max_output_tokens
}

/// Return the token count at which a new request should be compacted.
///
/// An explicitly configured threshold remains authoritative, but is clamped so
/// the configured output budget still fits inside the usable context window.
pub fn compaction_threshold(model: Option<&str>, reported_context_window: u64) -> Option<u64> {
    let limits = limits_for(model, reported_context_window);
    let context = limits.context_window?;
    let usable = limits.max_input_tokens.map_or(
        context.saturating_mul(USABLE_CONTEXT_PERCENT) / 100,
        |limit| limit.min(context),
    );
    let output_reserve = limits.max_output_tokens.unwrap_or_default();
    let output_safe = usable.saturating_sub(output_reserve);
    if output_safe == 0 {
        return Some(1);
    }

    let default_threshold = match context {
        1_000_000 => ONE_MILLION_COMPACTION_THRESHOLD,
        200_000 => TWO_HUNDRED_THOUSAND_COMPACTION_THRESHOLD,
        _ => context.saturating_mul(DEFAULT_AUTO_COMPACT_PERCENT) / 100,
    };
    let requested = limits.compaction_at_tokens.unwrap_or(default_threshold);
    Some(requested.min(output_safe).max(1))
}

fn positive_option(value: Option<u64>) -> Option<u64> {
    value.filter(|value| *value > 0)
}

fn positive_value(value: u64) -> Option<u64> {
    (value > 0).then_some(value)
}
