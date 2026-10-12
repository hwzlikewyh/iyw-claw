use super::context_window_config::ContextBudget;
use super::error::AcpError;
use crate::models::AgentType;

const KIMI_MIN_RESERVED_TOKENS: u64 = 1_000;

pub(crate) fn apply_file(budget: &ContextBudget) -> Result<(), AcpError> {
    let root = super::provider_overlay_files::active_profile_root(budget.agent)
        .map_err(AcpError::protocol)?;
    let path = root.join(if budget.agent == AgentType::Hermes {
        "config.yaml"
    } else {
        "config.toml"
    });
    let raw = super::provider_overlay_files::read_optional(&path).map_err(AcpError::protocol)?;
    let next = if budget.agent == AgentType::Hermes {
        patch_hermes(&raw, budget)?
    } else {
        patch_toml(&raw, budget)?
    };
    super::provider_overlay::write_if_changed(&path, &raw, &next).map_err(AcpError::protocol)
}

fn patch_toml(raw: &str, budget: &ContextBudget) -> Result<String, AcpError> {
    let mut value: toml::Value = if raw.trim().is_empty() {
        toml::Value::Table(Default::default())
    } else {
        toml::from_str(raw)
            .map_err(|error| AcpError::protocol(format!("Invalid SDK context TOML: {error}")))?
    };
    let path = if budget.agent == AgentType::KimiCode {
        "models"
    } else {
        "model"
    };
    let model = value
        .get_mut(path)
        .and_then(|models| models.get_mut(&budget.model))
        .and_then(toml::Value::as_table_mut)
        .ok_or_else(|| AcpError::protocol("Selected native SDK model configuration is missing"))?;
    let key = if budget.agent == AgentType::KimiCode {
        "max_context_size"
    } else {
        "context_window"
    };
    model.insert(key.into(), toml::Value::Integer(budget.window as i64));
    if budget.agent == AgentType::KimiCode {
        let root = value
            .as_table_mut()
            .ok_or_else(|| AcpError::protocol("SDK config is not a table"))?;
        let loop_control = root
            .entry("loop_control")
            .or_insert_with(|| toml::Value::Table(Default::default()))
            .as_table_mut()
            .ok_or_else(|| AcpError::protocol("Kimi loop control is not a table"))?;
        loop_control.insert(
            "reserved_context_size".into(),
            (budget
                .window
                .saturating_sub(budget.compact)
                .max(KIMI_MIN_RESERVED_TOKENS) as i64)
                .into(),
        );
        loop_control.insert(
            "compaction_trigger_ratio".into(),
            toml::Value::Float((budget.compact as f64 / budget.window as f64).clamp(0.5, 0.99)),
        );
    }
    toml::to_string_pretty(&value).map_err(|error| AcpError::protocol(error.to_string()))
}

fn patch_hermes(raw: &str, budget: &ContextBudget) -> Result<String, AcpError> {
    let mut value: serde_yaml::Value = if raw.trim().is_empty() {
        serde_yaml::Value::Mapping(Default::default())
    } else {
        serde_yaml::from_str(raw)
            .map_err(|error| AcpError::protocol(format!("Invalid Hermes config: {error}")))?
    };
    let root = value
        .as_mapping_mut()
        .ok_or_else(|| AcpError::protocol("Hermes config must be a mapping"))?;
    for (section, fields) in [
        (
            "model",
            vec![("context_length", serde_yaml::Value::from(budget.window))],
        ),
        (
            "compression",
            vec![
                ("enabled", true.into()),
                (
                    "threshold",
                    serde_yaml::Value::from(budget.compact as f64 / budget.window as f64),
                ),
                ("threshold_tokens", budget.compact.into()),
            ],
        ),
    ] {
        let entry = root
            .entry(section.into())
            .or_insert_with(|| serde_yaml::Value::Mapping(Default::default()))
            .as_mapping_mut()
            .ok_or_else(|| AcpError::protocol("Hermes budget section must be a mapping"))?;
        for (key, value) in fields {
            entry.insert(key.into(), value);
        }
    }
    serde_yaml::to_string(&value).map_err(|error| AcpError::protocol(error.to_string()))
}
