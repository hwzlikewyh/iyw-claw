use serde_json::{json, Value};

use crate::UpstreamError;

/// 与外置适配器使用同一主进程配置投影，但不修改工作进程的全局环境。
pub(super) fn environment_overrides(
    config_json: Option<&str>,
    home: Option<&std::path::Path>,
) -> Result<Value, UpstreamError> {
    let raw = match config_json {
        Some(raw) => raw.to_string(),
        None => match std::env::var("CODEX_CONFIG") {
            Ok(raw) => raw,
            Err(_) => return Ok(json!([])),
        },
    };
    if raw.is_empty() {
        return Ok(json!([]));
    }
    let mut value: Value =
        serde_json::from_str(&raw).map_err(|_| invalid("CODEX_CONFIG is not valid JSON"))?;
    if let Some(home) = home {
        let config = serde_json::from_value(normalize(&value))
            .map_err(|_| invalid("CODEX_CONFIG has unsupported TOML values"))?;
        let resolved = codex_config::loader::resolve_relative_paths_in_config_toml(config, home)
            .map_err(|_| invalid("CODEX_CONFIG has invalid relative paths"))?;
        value = serde_json::to_value(resolved)
            .map_err(|_| invalid("CODEX_CONFIG cannot be projected"))?;
    }
    let object = value
        .as_object()
        .ok_or_else(|| invalid("CODEX_CONFIG must be an object"))?;
    // CLI 覆盖键支持点号；值按上游 TOML 类型反序列化，不能拼接成命令行。
    Ok(Value::Array(
        object
            .iter()
            .map(|(key, value)| json!([key, normalize(value)]))
            .collect(),
    ))
}

fn normalize(value: &Value) -> Value {
    // 与锁定上游 json_to_toml 的 null 语义一致，TOML 本身不支持 null。
    match value {
        Value::Null => json!(""),
        Value::Array(values) => Value::Array(values.iter().map(normalize).collect()),
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), normalize(value)))
                .collect(),
        ),
        Value::Number(number) if number.as_i64().is_none() => number
            .as_f64()
            .map(|number| json!(number))
            .unwrap_or_else(|| json!(number.to_string())),
        _ => value.clone(),
    }
}

pub(super) fn invalid(message: &str) -> UpstreamError {
    UpstreamError::Start(message.to_string())
}
