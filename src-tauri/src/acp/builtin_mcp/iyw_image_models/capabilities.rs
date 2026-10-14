use serde_json::{json, Map, Value};

const MAX_NUMBER: u64 = 1 << 30;
const NUMBER_FIELDS: [&str; 2] = ["min", "max"];
const CUSTOM_FIELDS: [&str; 5] = ["min_edge", "max_edge", "step", "min_pixels", "max_pixels"];
const LIMIT_FIELDS: [&str; 5] = [
    "max_outputs",
    "max_prompt_chars",
    "max_images",
    "max_masks",
    "max_input_bytes",
];
const PARAMETER_FIELDS: [&str; 14] = [
    "quality",
    "size",
    "n",
    "background",
    "output_format",
    "output_compression",
    "response_format",
    "moderation",
    "input_fidelity",
    "style",
    "user",
    "mask",
    "stream",
    "partial_images",
];

// 只投影业务枚举与有限数字，不透传上游扩展、价格、URL 或内部标识。
pub(in crate::acp::builtin_mcp) fn summary(model: &Value) -> Value {
    let Some(capabilities) = model.get("image_capabilities") else {
        return Value::Null;
    };
    if capabilities.get("version").and_then(Value::as_u64) != Some(1) {
        return Value::Null;
    }
    let mut output = json!({"version": 1});
    for kind in ["generate", "edit"] {
        if let Some(operation) = capabilities.get(kind).filter(|value| value.is_object()) {
            output[kind] = operation_summary(operation);
        }
    }
    output
}

fn operation_summary(operation: &Value) -> Value {
    let mut output = json!({});
    if let Some(parameters) = operation.get("parameters").and_then(Value::as_object) {
        let projected = PARAMETER_FIELDS
            .iter()
            .filter_map(|key| {
                let rule = parameters.get(*key)?.as_object()?;
                Some((key.to_string(), parameter_summary(key, rule)))
            })
            .collect::<Map<_, _>>();
        output["parameters"] = Value::Object(projected);
    }
    if let Some(limits) = operation.get("limits").and_then(Value::as_object) {
        let mut projected = numeric_fields(limits, &LIMIT_FIELDS);
        copy_enums(
            limits,
            &mut projected,
            ("input_formats", &["png", "jpeg", "webp"]),
        );
        output["limits"] = Value::Object(projected);
    }
    output
}

fn parameter_summary(key: &str, rule: &Map<String, Value>) -> Value {
    let mut output = numeric_fields(rule, &NUMBER_FIELDS);
    if let Some(supported) = rule.get("supported").and_then(Value::as_bool) {
        output.insert("supported".into(), json!(supported));
    }
    copy_enums(rule, &mut output, ("values", allowed_values(key)));
    if key == "size" {
        if let Some(sizes) = rule.get("sizes").and_then(Value::as_array) {
            let sizes = sizes
                .iter()
                .filter_map(Value::as_str)
                .filter(|size| *size == "auto" || dimensions(size).is_some())
                .take(64)
                .collect::<Vec<_>>();
            if !sizes.is_empty() {
                output.insert("sizes".into(), json!(sizes));
            }
        }
        if let Some(custom) = rule.get("custom").and_then(Value::as_object) {
            output.insert("custom".into(), custom_summary(custom));
        }
    }
    Value::Object(output)
}

fn numeric_fields(source: &Map<String, Value>, fields: &[&str]) -> Map<String, Value> {
    fields
        .iter()
        .filter_map(|key| {
            let number = source.get(*key)?.as_u64()?;
            (number <= MAX_NUMBER).then(|| (key.to_string(), json!(number)))
        })
        .collect()
}

fn custom_summary(custom: &Map<String, Value>) -> Value {
    let mut output = numeric_fields(custom, &CUSTOM_FIELDS);
    if let Some(ratio) = custom.get("max_aspect_ratio").and_then(Value::as_f64) {
        if ratio.is_finite() && (1.0..=MAX_NUMBER as f64).contains(&ratio) {
            output.insert("max_aspect_ratio".into(), json!(ratio));
        }
    }
    if output.get("step").and_then(Value::as_u64) == Some(0) {
        output.remove("step");
    }
    Value::Object(output)
}

fn copy_enums(
    source: &Map<String, Value>,
    target: &mut Map<String, Value>,
    (field, allowed): (&str, &[&str]),
) {
    let Some(values) = source.get(field).and_then(Value::as_array) else {
        return;
    };
    let values = values
        .iter()
        .filter_map(Value::as_str)
        .filter(|value| allowed.contains(value))
        .take(allowed.len())
        .collect::<Vec<_>>();
    if !values.is_empty() {
        target.insert(field.into(), json!(values));
    }
}

fn allowed_values(key: &str) -> &'static [&'static str] {
    match key {
        "quality" => &["auto", "low", "medium", "high", "standard", "hd"],
        "background" => &["auto", "opaque", "transparent"],
        "output_format" => &["png", "jpeg", "webp"],
        "response_format" => &["url", "b64_json"],
        "moderation" => &["auto", "low"],
        "input_fidelity" => &["low", "high"],
        "style" => &["vivid", "natural"],
        _ => &[],
    }
}

pub(in crate::acp::builtin_mcp) fn dimensions(size: &str) -> Option<(u64, u64)> {
    let (width, height) = size.split_once('x')?;
    let (width, height) = (width.parse::<u64>().ok()?, height.parse::<u64>().ok()?);
    (width > 0 && height > 0 && width <= MAX_NUMBER && height <= MAX_NUMBER)
        .then_some((width, height))
}
