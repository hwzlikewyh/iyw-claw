use std::path::Path;

use serde_json::{json, Value};

use super::context_window_config::ContextBudget;
use super::error::AcpError;
use crate::models::AgentType;

pub(crate) fn apply_file(budget: &ContextBudget) -> Result<(), AcpError> {
    let root = super::provider_overlay_files::active_profile_root(budget.agent)
        .map_err(AcpError::protocol)?;
    let file = match budget.agent {
        AgentType::Gemini | AgentType::Pi => "settings.json",
        AgentType::Cline => "globalState.json",
        AgentType::OpenClaw => "openclaw.json",
        _ => {
            return Err(AcpError::protocol(
                "Unsupported native JSON context configuration",
            ))
        }
    };
    patch_file(&root.join(file), |value| patch(value, budget))?;
    if budget.agent == AgentType::Pi {
        patch_file(&root.join("models.json"), |value| {
            patch_provider_models(value, budget)
        })?;
    }
    Ok(())
}

fn patch_file(
    path: &Path,
    mutate: impl FnOnce(&mut Value) -> Result<(), AcpError>,
) -> Result<(), AcpError> {
    let raw = super::provider_overlay_files::read_optional(path).map_err(AcpError::protocol)?;
    let mut value: Value = if raw.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(&raw)
            .map_err(|error| AcpError::protocol(format!("Invalid native SDK JSON: {error}")))?
    };
    if !value.is_object() {
        return Err(AcpError::protocol("Native SDK JSON must be an object"));
    }
    mutate(&mut value)?;
    let next = serde_json::to_string_pretty(&value)
        .map_err(|error| AcpError::protocol(error.to_string()))?;
    super::provider_overlay::write_if_changed(path, &raw, &(next + "\n"))
        .map_err(AcpError::protocol)
}

fn patch(value: &mut Value, budget: &ContextBudget) -> Result<(), AcpError> {
    match budget.agent {
        AgentType::Gemini => {
            set_value(
                value,
                &["model", "compressionThreshold"],
                json!(budget.compact as f64 / budget.maximum as f64),
            )?;
        }
        AgentType::OpenClaw => {
            set_value(
                value,
                &["agents", "defaults", "contextTokens"],
                budget.window.into(),
            )?;
            patch_provider_models(value, budget)?;
        }
        AgentType::Cline => {
            for key in [
                "openAiModelInfo",
                "actModeOpenAiModelInfo",
                "planModeOpenAiModelInfo",
            ] {
                set_value(value, &[key, "contextWindow"], budget.window.into())?;
            }
        }
        AgentType::Pi => {
            set_value(value, &["compaction", "enabled"], true.into())?;
            set_value(
                value,
                &["compaction", "reserveTokens"],
                budget.window.saturating_sub(budget.compact).into(),
            )?;
        }
        _ => return Err(AcpError::protocol("Unknown native context budget adapter")),
    }
    Ok(())
}

fn patch_provider_models(value: &mut Value, budget: &ContextBudget) -> Result<(), AcpError> {
    let provider = super::provider_overlay::MANAGED_PROVIDER_ID;
    let models = if budget.agent == AgentType::OpenClaw {
        value.pointer_mut(&format!("/models/providers/{provider}/models"))
    } else {
        value.pointer_mut(&format!("/providers/{provider}/models"))
    };
    let models = models
        .and_then(Value::as_array_mut)
        .ok_or_else(|| AcpError::protocol("Managed SDK model catalog is missing"))?;
    let model = models
        .iter_mut()
        .find(|model| model["id"].as_str() == Some(&budget.model))
        .ok_or_else(|| {
            AcpError::protocol("Selected model is absent from the native SDK catalog")
        })?;
    model["contextWindow"] = budget.window.into();
    Ok(())
}

pub(crate) fn patch_opencode(value: &mut Value, budget: &ContextBudget) -> Result<(), AcpError> {
    let provider = super::provider_overlay::MANAGED_PROVIDER_ID;
    set_value(
        value,
        &[
            "provider",
            provider,
            "models",
            &budget.model,
            "limit",
            "context",
        ],
        budget.window.into(),
    )?;
    set_value(
        value,
        &[
            "provider",
            provider,
            "models",
            &budget.model,
            "limit",
            "input",
        ],
        budget.input.into(),
    )?;
    set_value(value, &["compaction", "auto"], true.into())?;
    set_value(
        value,
        &["compaction", "reserved"],
        budget.window.saturating_sub(budget.compact).into(),
    )?;
    Ok(())
}

fn set_value(root: &mut Value, path: &[&str], value: Value) -> Result<(), AcpError> {
    let (key, parents) = path
        .split_last()
        .ok_or_else(|| AcpError::protocol("SDK context setting path is empty"))?;
    let mut cursor = root;
    for parent in parents {
        let object = cursor
            .as_object_mut()
            .ok_or_else(|| AcpError::protocol("SDK context setting parent must be an object"))?;
        cursor = object.entry(*parent).or_insert_with(|| json!({}));
        if cursor.is_null() {
            *cursor = json!({});
        }
    }
    cursor
        .as_object_mut()
        .ok_or_else(|| AcpError::protocol("SDK context setting parent must be an object"))?
        .insert((*key).into(), value);
    Ok(())
}
