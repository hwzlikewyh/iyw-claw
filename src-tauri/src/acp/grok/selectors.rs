use std::collections::BTreeMap;

use crate::acp::provider_overlay::{MANAGED_DEFAULT_MODEL, MANAGED_MODEL_IDS};
use crate::acp::types::{
    SessionConfigKindInfo, SessionConfigOptionInfo, SessionConfigSelectInfo,
    SessionConfigSelectOptionInfo,
};

pub(crate) type EffortSpecs = BTreeMap<String, EffortSpec>;

#[derive(Debug, Clone, Default)]
pub(crate) struct EffortSpec {
    options: Vec<(String, Option<String>)>,
    default: Option<String>,
    supports: bool,
}

fn effort_label(id: &str) -> &str {
    match id {
        "low" => "Low",
        "medium" => "Medium",
        "high" => "High",
        "xhigh" => "Max",
        other => other,
    }
}

fn effort_description(id: &str) -> Option<&'static str> {
    match id {
        "low" => Some("Quick, fast responses"),
        "medium" => Some("Balanced speed and quality"),
        "high" => Some("Extensive reasoning for high quality"),
        "xhigh" => Some("Maximum reasoning for the most complex tasks"),
        _ => None,
    }
}

pub(crate) fn parse_effort_specs(models: Option<&serde_json::Value>) -> EffortSpecs {
    let mut specs = EffortSpecs::new();
    let Some(models) = models
        .and_then(|value| value.get("availableModels"))
        .and_then(serde_json::Value::as_array)
    else {
        return specs;
    };
    for model in models {
        let Some(model_id) = model.get("modelId").and_then(serde_json::Value::as_str) else {
            continue;
        };
        if !MANAGED_MODEL_IDS.contains(&model_id) {
            continue;
        }
        let meta = model.get("_meta");
        let supports = meta
            .and_then(|value| value.get("supportsReasoningEffort"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let default = meta
            .and_then(|value| value.get("reasoningEffort"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let options = meta
            .and_then(|value| value.get("reasoningEfforts"))
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|effort| {
                let id = effort.get("id")?.as_str()?.to_string();
                let description = effort
                    .get("description")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                Some((id, description))
            })
            .collect();
        specs.insert(
            model_id.to_string(),
            EffortSpec {
                options,
                default,
                supports,
            },
        );
    }
    specs
}

fn build_effort_option(model_id: &str, specs: &EffortSpecs) -> Option<SessionConfigOptionInfo> {
    let spec = specs.get(model_id)?;
    if !spec.supports {
        return None;
    }
    let mut options: Vec<_> = spec
        .options
        .iter()
        .map(|(id, description)| SessionConfigSelectOptionInfo {
            value: id.clone(),
            name: effort_label(id).to_string(),
            description: description
                .clone()
                .or_else(|| effort_description(id).map(str::to_string)),
        })
        .collect();
    if let Some(default) = &spec.default {
        if !options.iter().any(|option| option.value == *default) {
            options.insert(
                0,
                SessionConfigSelectOptionInfo {
                    value: default.clone(),
                    name: effort_label(default).to_string(),
                    description: effort_description(default).map(str::to_string),
                },
            );
        }
    }
    select_option(
        "reasoning_effort",
        "Reasoning effort",
        "mode",
        spec.default
            .clone()
            .or_else(|| options.first().map(|option| option.value.clone()))?,
        options,
    )
}

fn select_option(
    id: &str,
    name: &str,
    category: &str,
    current_value: String,
    options: Vec<SessionConfigSelectOptionInfo>,
) -> Option<SessionConfigOptionInfo> {
    (!options.is_empty()).then(|| SessionConfigOptionInfo {
        id: id.to_string(),
        name: name.to_string(),
        description: None,
        category: Some(category.to_string()),
        kind: SessionConfigKindInfo::Select(SessionConfigSelectInfo {
            current_value,
            options,
            groups: Vec::new(),
        }),
    })
}

pub(crate) fn synthesize_options(
    meta: Option<&serde_json::Map<String, serde_json::Value>>,
    specs: &EffortSpecs,
) -> Option<Vec<SessionConfigOptionInfo>> {
    let raw_options = meta
        .and_then(|meta| meta.get("x.ai/sessionConfig"))
        .and_then(|value| value.get("options"))
        .and_then(serde_json::Value::as_array);
    let label_for = |model_id: &str| {
        raw_options
            .into_iter()
            .flatten()
            .find(|option| option.get("id").and_then(serde_json::Value::as_str) == Some(model_id))
            .and_then(|option| option.get("label"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or(model_id)
            .to_string()
    };
    let selected = raw_options
        .into_iter()
        .flatten()
        .find(|option| {
            option.get("selected").and_then(serde_json::Value::as_bool) == Some(true)
                && option
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|id| MANAGED_MODEL_IDS.contains(&id))
        })
        .and_then(|option| option.get("id"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or(MANAGED_DEFAULT_MODEL)
        .to_string();
    let model_options = MANAGED_MODEL_IDS
        .iter()
        .map(|model_id| SessionConfigSelectOptionInfo {
            value: (*model_id).to_string(),
            name: label_for(model_id),
            description: None,
        })
        .collect();
    let mut result = vec![select_option(
        "model",
        "Model",
        "model",
        selected.clone(),
        model_options,
    )?];
    if let Some(effort) = build_effort_option(&selected, specs) {
        result.push(effort);
    }
    Some(result)
}

pub(crate) fn set_effort_selector_for_model(
    options: &mut Vec<SessionConfigOptionInfo>,
    model_id: &str,
    specs: &EffortSpecs,
) {
    options.retain(|option| option.id != "reasoning_effort");
    if let Some(effort) = build_effort_option(model_id, specs) {
        options.push(effort);
    }
}

pub(crate) fn build_set_model_params(
    session_id: &str,
    model_id: &str,
    reasoning_effort: Option<&str>,
) -> serde_json::Value {
    let mut params = serde_json::json!({"sessionId": session_id, "modelId": model_id});
    if let Some(effort) = reasoning_effort {
        params["_meta"] = serde_json::json!({"reasoningEffort": effort});
    }
    params
}
