use super::error::AcpError;
use super::types::{
    SessionConfigKindInfo, SessionConfigOptionInfo, SessionConfigSelectInfo,
    SessionConfigSelectOptionInfo,
};
use crate::models::AgentType;

pub(crate) const CONFIG_ID: &str = "compaction_threshold";

pub(crate) fn percents(agent: AgentType, model: &str) -> Vec<u64> {
    let mut values = super::model_catalog::compaction_threshold_percents_for(agent, model);
    values.retain(|value| (1..=100).contains(value));
    values.sort_unstable();
    values.dedup();
    values
}

pub(crate) fn selected_percent(
    agent: AgentType,
    model: &str,
    value: &str,
) -> Result<Option<u64>, AcpError> {
    if value == "auto" {
        return Ok(None);
    }
    value
        .parse::<u64>()
        .ok()
        .filter(|percent| percents(agent, model).contains(percent))
        .map(Some)
        .ok_or_else(|| {
            AcpError::protocol("Selected compaction threshold is not an enabled backend tier")
        })
}

pub(crate) fn option(agent: AgentType, model: &str, value: &str) -> SessionConfigOptionInfo {
    let mut options = vec![SessionConfigSelectOptionInfo {
        value: "auto".into(),
        name: "Auto".into(),
        description: Some(model.into()),
    }];
    options.extend(
        percents(agent, model)
            .iter()
            .map(|percent| SessionConfigSelectOptionInfo {
                value: percent.to_string(),
                name: format!("{percent}%"),
                description: None,
            }),
    );
    SessionConfigOptionInfo {
        id: CONFIG_ID.into(),
        name: "Auto-compaction threshold".into(),
        description: None,
        category: Some("model_config".into()),
        kind: SessionConfigKindInfo::Select(SessionConfigSelectInfo {
            current_value: value.into(),
            options,
            groups: Vec::new(),
        }),
    }
}

pub(crate) fn is_budget_config(id: &str) -> bool {
    id == CONFIG_ID || id == super::context_window_config::CONFIG_ID
}
