use std::collections::HashSet;

use sacp::schema::{
    SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory, SessionConfigSelectOptions,
};

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
