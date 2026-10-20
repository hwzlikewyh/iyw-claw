use super::market_types::OptionSpec;
use crate::app_error::AppCommandError;
use serde_json::{Map, Value};

pub(crate) fn hydrate_spec(spec: &Value) -> Result<Value, AppCommandError> {
    let mut resolved = spec.clone();
    if let Some(key) = spec.get("iyw_secret_ref").and_then(Value::as_str) {
        let raw = crate::keyring_store::get_connector_secret(key)
            .ok_or_else(|| AppCommandError::configuration_invalid("连接器凭据缺失，请重新配置"))?;
        let values: Value = serde_json::from_str(&raw)
            .map_err(|_| AppCommandError::configuration_invalid("连接器凭据格式无效"))?;
        resolved = values
            .get("resolved_spec")
            .filter(|value| value.is_object())
            .cloned()
            .ok_or_else(|| {
                AppCommandError::configuration_invalid("连接器凭据格式无效，请重新配置")
            })?;
    }
    if let Some(map) = resolved.as_object_mut() {
        map.remove("iyw_secret_ref");
        map.remove("iyw_market");
    }
    Ok(resolved)
}

pub(super) fn parameter_values(
    option: &OptionSpec,
    input: &Map<String, Value>,
) -> Result<Map<String, Value>, AppCommandError> {
    let mut values = Map::new();
    for field in &option.parameters {
        let value = input
            .get(&field.key)
            .cloned()
            .or_else(|| field.default_value.clone())
            .unwrap_or(Value::String(String::new()));
        let missing =
            value.is_null() || value.as_str().is_some_and(|value| value.trim().is_empty());
        if field.required && missing {
            return Err(AppCommandError::invalid_input(format!(
                "请填写 {}",
                field.label
            )));
        }
        if !missing && !valid_value(&value, &field.kind) {
            return Err(AppCommandError::invalid_input(format!(
                "{} 的参数类型无效",
                field.label
            )));
        }
        if !field.enum_values.is_empty()
            && !missing
            && !field
                .enum_values
                .iter()
                .any(|item| value.as_str() == Some(item))
        {
            return Err(AppCommandError::invalid_input(format!(
                "{} 的选项无效",
                field.label
            )));
        }
        values.insert(field.key.clone(), value);
    }
    Ok(values)
}

fn valid_value(value: &Value, kind: &str) -> bool {
    match kind {
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "number" => value.is_number(),
        "integer" => value.is_i64() || value.is_u64(),
        "json" => true,
        _ => false,
    }
}

pub(super) fn substitute(
    value: &mut Value,
    parameters: &Map<String, Value>,
    allow_unresolved: bool,
) -> Result<(), AppCommandError> {
    match value {
        Value::String(text) => {
            *text = substitute_text(text, parameters, allow_unresolved)?;
        }
        Value::Array(items) => {
            for item in items {
                substitute(item, parameters, allow_unresolved)?;
            }
        }
        Value::Object(items) => {
            for item in items.values_mut() {
                substitute(item, parameters, allow_unresolved)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn substitute_text(
    text: &str,
    parameters: &Map<String, Value>,
    allow_unresolved: bool,
) -> Result<String, AppCommandError> {
    let mut output = String::new();
    let mut remaining = text;
    while let Some(start) = remaining.find("${") {
        output.push_str(&remaining[..start]);
        let Some(end) = remaining[start..].find('}').map(|end| start + end) else {
            return Err(AppCommandError::invalid_input("连接器参数模板无效"));
        };
        let key = &remaining[start + 2..end];
        if let Some(value) = parameters.get(key) {
            output.push_str(
                &value
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| value.to_string()),
            );
        } else if allow_unresolved {
            output.push_str(&remaining[start..=end]);
        } else {
            return Err(AppCommandError::invalid_input("连接器仍有未填写的配置参数"));
        }
        remaining = &remaining[end + 1..];
    }
    output.push_str(remaining);
    Ok(output)
}
