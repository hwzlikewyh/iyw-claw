use std::collections::HashSet;

use sacp::schema::{
    SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory, SessionConfigSelectOptions,
};

use super::provider_overlay::{managed_model_ids_for, MANAGED_PROVIDER_ID};
use super::types::{SessionConfigKindInfo, SessionConfigOptionInfo};
use crate::models::AgentType;

pub(crate) fn canonical_model_id(value: &str) -> &str {
    value
        .strip_prefix(MANAGED_PROVIDER_ID)
        .and_then(|suffix| suffix.strip_prefix('/'))
        .unwrap_or(value)
}

pub(crate) fn canonical_model_id_for_agent(agent: AgentType, value: &str) -> &str {
    if agent == AgentType::Hermes {
        return value.strip_prefix("custom:").unwrap_or(value);
    }
    if matches!(agent, AgentType::OpenCode | AgentType::Pi) {
        return canonical_model_id(value);
    }
    value
}

fn uses_provider_model_ids(agent_type: AgentType) -> bool {
    matches!(
        agent_type,
        AgentType::OpenCode | AgentType::Pi | AgentType::Hermes
    )
}

pub(crate) fn model_value_for_agent(agent_type: AgentType, value: String) -> String {
    if uses_provider_model_ids(agent_type)
        && managed_model_ids_for(agent_type).contains(&value.as_str())
    {
        if agent_type == AgentType::Hermes {
            return format!("custom:{value}");
        }
        return format!("{MANAGED_PROVIDER_ID}/{value}");
    }
    value
}

pub(crate) fn project_model_options(
    agent_type: AgentType,
    options: &mut [SessionConfigOptionInfo],
) {
    if !uses_provider_model_ids(agent_type) {
        return;
    }
    for option in options
        .iter_mut()
        .filter(|option| option.id == "model" || option.category.as_deref() == Some("model"))
    {
        let SessionConfigKindInfo::Select(select) = &mut option.kind;
        select.current_value =
            canonical_model_id_for_agent(agent_type, &select.current_value).to_string();
        for value in select.options.iter_mut().chain(
            select
                .groups
                .iter_mut()
                .flat_map(|group| &mut group.options),
        ) {
            value.value = canonical_model_id_for_agent(agent_type, &value.value).to_string();
        }
    }
}

fn preferred_config_category(config_id: &str) -> Option<SessionConfigOptionCategory> {
    match config_id {
        "mode" => Some(SessionConfigOptionCategory::Mode),
        "model" => Some(SessionConfigOptionCategory::Model),
        "reasoning_effort" => Some(SessionConfigOptionCategory::ThoughtLevel),
        _ => None,
    }
}

fn select_values(select: &sacp::schema::SessionConfigSelect) -> Vec<String> {
    match &select.options {
        SessionConfigSelectOptions::Ungrouped(options) => {
            options.iter().map(|item| item.value.to_string()).collect()
        }
        SessionConfigSelectOptions::Grouped(groups) => groups
            .iter()
            .flat_map(|group| group.options.iter())
            .map(|item| item.value.to_string())
            .collect(),
        _ => Vec::new(),
    }
}

fn resolve_value(
    option: &SessionConfigOption,
    preferred_id: &str,
    preferred_value: &str,
) -> Option<String> {
    let SessionConfigKind::Select(select) = &option.kind else {
        return None;
    };
    let values = select_values(select);
    if values.iter().any(|value| value == preferred_value) {
        return Some(preferred_value.to_string());
    }
    // 界面使用网关模型 ID，协议请求必须保留 Agent 实际公布的供应商前缀。
    if preferred_id == "model"
        || option.category.as_ref() == Some(&SessionConfigOptionCategory::Model)
    {
        return values
            .into_iter()
            .find(|value| canonical_model_id(value) == canonical_model_id(preferred_value));
    }

    let is_thought_level = matches!(
        option.category.as_ref(),
        Some(SessionConfigOptionCategory::ThoughtLevel)
    ) || preferred_id == "reasoning_effort";
    let unique_values = values.iter().map(String::as_str).collect::<HashSet<_>>();
    let is_binary_switch =
        unique_values.len() == 2 && unique_values.contains("off") && unique_values.contains("on");
    (is_thought_level && is_binary_switch).then(|| {
        if preferred_value == "off" {
            "off"
        } else {
            "on"
        }
        .to_string()
    })
}

pub(crate) fn resolve_preferred_session_config(
    options: &[SessionConfigOption],
    preferred_id: &str,
    preferred_value: &str,
) -> Option<(String, String)> {
    if let Some(option) = options
        .iter()
        .find(|option| option.id.to_string() == preferred_id)
    {
        let value = resolve_value(option, preferred_id, preferred_value)?;
        return Some((option.id.to_string(), value));
    }

    let category = preferred_config_category(preferred_id)?;
    let candidates = options
        .iter()
        .filter(|option| option.category.as_ref() == Some(&category))
        .collect::<Vec<_>>();
    if candidates.len() == 1 {
        let option = candidates[0];
        let value = resolve_value(option, preferred_id, preferred_value)?;
        return Some((option.id.to_string(), value));
    }

    (preferred_id == "mode" && candidates.is_empty())
        .then(|| (preferred_id.to_string(), preferred_value.to_string()))
}
