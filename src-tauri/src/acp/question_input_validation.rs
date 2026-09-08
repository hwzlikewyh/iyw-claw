use serde_json::Value;

pub(super) fn validate_schema(schema: &Value) -> Result<(), String> {
    if !matches!(
        schema["type"].as_str(),
        Some("string" | "boolean" | "number" | "integer" | "array")
    ) {
        return Err("不支持此表单字段类型".into());
    }
    if let Some(pattern) = schema["pattern"].as_str() {
        regex::Regex::new(pattern).map_err(|_| "表单包含不受支持的正则约束")?;
    }
    for options in [
        schema.get("enum"),
        schema.get("oneOf"),
        schema.pointer("/items/enum"),
        schema.pointer("/items/anyOf"),
    ] {
        if options.and_then(Value::as_array).is_some_and(Vec::is_empty) {
            return Err("表单枚举选项不能为空".into());
        }
    }
    for (min, max) in [
        ("minLength", "maxLength"),
        ("minimum", "maximum"),
        ("minItems", "maxItems"),
    ] {
        if schema[min]
            .as_f64()
            .zip(schema[max].as_f64())
            .is_some_and(|(min, max)| min > max)
        {
            return Err("表单字段的最小值超过最大值".into());
        }
    }
    Ok(())
}

pub(super) fn validate(schema: &Value, value: &Value, allow_other: bool) -> Result<(), String> {
    match value {
        Value::String(text) => validate_string(schema, text, allow_other),
        Value::Array(values) => validate_array(schema, values),
        Value::Number(_) => validate_number(schema, value),
        Value::Bool(_) => Ok(()),
        _ => Err("表单答案类型无效".into()),
    }
}

fn validate_string(schema: &Value, text: &str, allow_other: bool) -> Result<(), String> {
    validate_count(schema, text.chars().count(), ("minLength", "maxLength"))?;
    if !allow_other && !allowed_string(schema, text) {
        return Err("请选择有效选项".into());
    }
    if let Some(pattern) = schema["pattern"].as_str() {
        let pattern = regex::Regex::new(pattern).map_err(|_| "表单正则约束无效")?;
        if !pattern.is_match(text) {
            return Err("输入不符合要求的格式".into());
        }
    }
    let valid = match schema["format"].as_str() {
        Some("email") => email(text),
        Some("uri") => reqwest::Url::parse(text).is_ok(),
        Some("date") => {
            text.len() == 10 && chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d").is_ok()
        }
        Some("date-time") => chrono::DateTime::parse_from_rfc3339(text).is_ok(),
        None => true,
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err("输入不符合字段要求的格式".into())
    }
}

fn validate_array(schema: &Value, values: &[Value]) -> Result<(), String> {
    validate_count(schema, values.len(), ("minItems", "maxItems"))?;
    for value in values {
        let text = value.as_str().ok_or("多选字段必须是字符串")?;
        if !allowed_string(&schema["items"], text) {
            return Err("请选择有效选项".into());
        }
    }
    Ok(())
}

fn allowed_string(schema: &Value, text: &str) -> bool {
    let options = schema["oneOf"]
        .as_array()
        .or_else(|| schema["anyOf"].as_array());
    if let Some(options) = options {
        return options.iter().any(|option| option["const"] == text);
    }
    schema["enum"]
        .as_array()
        .is_none_or(|values| values.iter().any(|value| value == text))
}

fn validate_count(schema: &Value, length: usize, keys: (&str, &str)) -> Result<(), String> {
    if let Some(min) = schema[keys.0].as_u64().filter(|min| (length as u64) < *min) {
        return Err(format!("数量或长度不能少于 {min}"));
    }
    if let Some(max) = schema[keys.1].as_u64().filter(|max| (length as u64) > *max) {
        return Err(format!("数量或长度不能超过 {max}"));
    }
    Ok(())
}

fn validate_number(schema: &Value, value: &Value) -> Result<(), String> {
    if let Some(number) = value.as_i64().filter(|_| schema["type"] == "integer") {
        if schema["minimum"].as_i64().is_some_and(|min| number < min)
            || schema["maximum"].as_i64().is_some_and(|max| number > max)
        {
            return Err("整数超出字段允许的范围".into());
        }
        if schema["minimum"].is_null() || schema["minimum"].as_i64().is_some() {
            if schema["maximum"].is_null() || schema["maximum"].as_i64().is_some() {
                return Ok(());
            }
        }
    }
    let number = value.as_f64().ok_or("请输入有效数字")?;
    if schema["minimum"].as_f64().is_some_and(|min| number < min)
        || schema["maximum"].as_f64().is_some_and(|max| number > max)
    {
        return Err("数字超出字段允许的范围".into());
    }
    Ok(())
}

fn email(text: &str) -> bool {
    text.rsplit_once('@').is_some_and(|(local, domain)| {
        !local.is_empty()
            && !domain.is_empty()
            && !text.chars().any(char::is_whitespace)
            && !local.contains('@')
    })
}
