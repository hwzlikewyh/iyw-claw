use std::collections::BTreeMap;

use super::error::AcpError;
use super::types::{
    SessionConfigKindInfo, SessionConfigOptionInfo, SessionConfigSelectInfo,
    SessionConfigSelectOptionInfo,
};
use crate::models::AgentType;

pub(crate) const CONFIG_ID: &str = "context_window";
const COMPACT_PERCENT: u64 = 90;
const TOKENS_PER_K: f64 = 1_000.0;
const TOKENS_PER_M: f64 = 1_000_000.0;

pub(crate) struct ContextBudget {
    pub agent: AgentType,
    pub model: String,
    pub maximum: u64,
    pub window: u64,
    pub input: u64,
    pub compact: u64,
    pub value: String,
}

pub(crate) fn apply(
    environment: &mut BTreeMap<String, String>,
    agent: AgentType,
    preferences: &BTreeMap<String, String>,
) -> Result<Option<SessionConfigOptionInfo>, AcpError> {
    if !super::context_window_runtime::supports(agent) {
        return Ok(None);
    }
    let model = preferences
        .get("model")
        .cloned()
        .unwrap_or_else(|| super::model_catalog::default_model_for(agent).into());
    let Some(snapshot) = super::model_catalog::model_capabilities_for(agent, &model) else {
        return Ok(None);
    };
    let limits = snapshot.limits;
    let Some(maximum) = limits.context_window.filter(|value| *value > 0) else {
        return Ok(None);
    };
    let mut tiers = super::model_catalog::context_window_tiers_for(agent, &model);
    tiers.retain(|value| *value > 0 && *value <= maximum);
    tiers.sort_unstable();
    tiers.dedup();
    let automatic = automatic_window(limits, maximum);
    let value = preferences
        .get(CONFIG_ID)
        .map(String::as_str)
        .unwrap_or("auto");
    let window = selected_window(value, automatic, &tiers)?;
    let compact = compaction_threshold(limits, window, value)?;
    let budget = ContextBudget {
        agent,
        model,
        maximum,
        window,
        input: limits.max_input_tokens.unwrap_or(window).min(window),
        compact,
        value: value.into(),
    };
    super::context_window_runtime::apply(environment, &budget)?;
    tracing::info!(agent = %agent, model = %budget.model, selected = value, window, compact,
        "[ACP] applied native SDK context window policy");
    Ok(Some(context_option(&budget, &tiers, automatic)))
}

fn automatic_window(limits: super::model_catalog::ModelLimits, maximum: u64) -> u64 {
    limits
        .default_context_window
        .filter(|value| *value > 0)
        .unwrap_or(maximum)
        .min(maximum)
}

fn compaction_threshold(
    limits: super::model_catalog::ModelLimits,
    window: u64,
    value: &str,
) -> Result<u64, AcpError> {
    super::model_budget::threshold_for_limits(super::model_catalog::ModelLimits {
        context_window: Some(window),
        compaction_at_tokens: if value == "auto" {
            limits.compaction_at_tokens.filter(|value| *value > 0)
        } else {
            Some(window.saturating_mul(COMPACT_PERCENT) / 100)
        },
        ..limits
    })
    .ok_or_else(|| AcpError::protocol("Context budget is unavailable"))
}

fn selected_window(value: &str, automatic: u64, tiers: &[u64]) -> Result<u64, AcpError> {
    if value == "auto" {
        return Ok(automatic);
    }
    value
        .parse::<u64>()
        .ok()
        .filter(|tokens| tiers.contains(tokens))
        .ok_or_else(|| AcpError::protocol("Selected context window is not an enabled backend tier"))
}

fn context_option(
    budget: &ContextBudget,
    tiers: &[u64],
    automatic: u64,
) -> SessionConfigOptionInfo {
    let mut options = vec![SessionConfigSelectOptionInfo {
        value: "auto".into(),
        name: format!("Auto · {}", window_label(automatic)),
        description: Some(budget.model.clone()),
    }];
    options.extend(tiers.iter().map(|tokens| SessionConfigSelectOptionInfo {
        value: tokens.to_string(),
        name: window_label(*tokens),
        description: None,
    }));
    SessionConfigOptionInfo {
        id: CONFIG_ID.into(),
        name: "Context window".into(),
        description: None,
        category: Some("model_config".into()),
        kind: SessionConfigKindInfo::Select(SessionConfigSelectInfo {
            current_value: budget.value.clone(),
            options,
            groups: Vec::new(),
        }),
    }
}

fn window_label(tokens: u64) -> String {
    if tokens as f64 >= TOKENS_PER_M {
        format!("{}M", tokens as f64 / TOKENS_PER_M)
    } else {
        format!("{}K", tokens as f64 / TOKENS_PER_K)
    }
}
