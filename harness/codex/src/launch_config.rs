use serde_json::{json, Value};

use crate::UpstreamError;

/// 与外置适配器使用同一主进程配置投影，但不修改工作进程的全局环境。
pub(super) fn environment_overrides() -> Result<Value, UpstreamError> {
    let Some(raw) = std::env::var_os("CODEX_CONFIG") else {
        return Ok(json!([]));
    };
    let raw = raw
        .to_str()
        .ok_or_else(|| invalid("CODEX_CONFIG is not UTF-8"))?;
    let value: Value =
        serde_json::from_str(raw).map_err(|_| invalid("CODEX_CONFIG is not valid JSON"))?;
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
