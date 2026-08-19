use serde_json::Value;

pub(super) fn validate(schema: &Value, instance: &Value) -> Result<(), SchemaValidationError> {
    validate_at(schema, instance, "$")
}

fn validate_at(schema: &Value, instance: &Value, path: &str) -> Result<(), SchemaValidationError> {
    validate_type(schema, instance, path)?;
    validate_enum(schema, instance, path)?;
    match instance {
        Value::Object(object) => validate_object(schema, object, path)?,
        Value::Array(items) => validate_array(schema, items, path)?,
        Value::String(text) => validate_string(schema, text, path)?,
        Value::Number(number) => validate_number(schema, number, path)?,
        _ => {}
    }
    validate_not(schema, instance, path)?;
    validate_one_of(schema, instance, path)
}

fn validate_type(
    schema: &Value,
    instance: &Value,
    path: &str,
) -> Result<(), SchemaValidationError> {
    let Some(expected) = schema.get("type").and_then(Value::as_str) else {
        return Ok(());
    };
    let matches = match expected {
        "object" => instance.is_object(),
        "array" => instance.is_array(),
        "string" => instance.is_string(),
        "integer" => instance.as_i64().is_some() || instance.as_u64().is_some(),
        "number" => instance.is_number(),
        "boolean" => instance.is_boolean(),
        "null" => instance.is_null(),
        _ => true,
    };
    if matches {
        Ok(())
    } else {
        Err(error(path, format!("must be {expected}")))
    }
}

fn validate_enum(
    schema: &Value,
    instance: &Value,
    path: &str,
) -> Result<(), SchemaValidationError> {
    let Some(values) = schema.get("enum").and_then(Value::as_array) else {
        return Ok(());
    };
    if values.contains(instance) {
        Ok(())
    } else {
        Err(error(path, "must be one of the declared enum values"))
    }
}

fn validate_object(
    schema: &Value,
    object: &serde_json::Map<String, Value>,
    path: &str,
) -> Result<(), SchemaValidationError> {
    if let Some(minimum) = schema.get("minProperties").and_then(Value::as_u64) {
        if object.len() < minimum as usize {
            return Err(error(
                path,
                format!("must contain at least {minimum} properties"),
            ));
        }
    }
    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        for field in required.iter().filter_map(Value::as_str) {
            if !object.contains_key(field) {
                return Err(error(&child_path(path, field), "is required"));
            }
        }
    }
    let properties = schema.get("properties").and_then(Value::as_object);
    if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
        if let Some(field) = object
            .keys()
            .find(|field| properties.is_none_or(|properties| !properties.contains_key(*field)))
        {
            return Err(error(
                &child_path(path, field),
                "is not an allowed property",
            ));
        }
    }
    if let Some(properties) = properties {
        for (field, value) in object {
            if let Some(field_schema) = properties.get(field) {
                validate_at(field_schema, value, &child_path(path, field))?;
            }
        }
    }
    Ok(())
}

fn validate_array(
    schema: &Value,
    items: &[Value],
    path: &str,
) -> Result<(), SchemaValidationError> {
    validate_len(schema, items.len(), path, "minItems", "maxItems", "items")?;
    if let Some(item_schema) = schema.get("items") {
        for (index, item) in items.iter().enumerate() {
            validate_at(item_schema, item, &format!("{path}[{index}]"))?;
        }
    }
    Ok(())
}

fn validate_string(schema: &Value, text: &str, path: &str) -> Result<(), SchemaValidationError> {
    validate_len(
        schema,
        text.chars().count(),
        path,
        "minLength",
        "maxLength",
        "characters",
    )?;
    validate_format(schema, text, path)
}

fn validate_format(schema: &Value, text: &str, path: &str) -> Result<(), SchemaValidationError> {
    match schema.get("format").and_then(Value::as_str) {
        Some("date-time") if chrono::DateTime::parse_from_rfc3339(text).is_err() => {
            Err(error(path, "must be an RFC 3339 date-time"))
        }
        _ => Ok(()),
    }
}

fn validate_len(
    schema: &Value,
    length: usize,
    path: &str,
    minimum_key: &str,
    maximum_key: &str,
    unit: &str,
) -> Result<(), SchemaValidationError> {
    if let Some(minimum) = schema.get(minimum_key).and_then(Value::as_u64) {
        if length < minimum as usize {
            return Err(error(
                path,
                format!("must contain at least {minimum} {unit}"),
            ));
        }
    }
    if let Some(maximum) = schema.get(maximum_key).and_then(Value::as_u64) {
        if length > maximum as usize {
            return Err(error(
                path,
                format!("must contain at most {maximum} {unit}"),
            ));
        }
    }
    Ok(())
}

fn validate_number(
    schema: &Value,
    number: &serde_json::Number,
    path: &str,
) -> Result<(), SchemaValidationError> {
    let Some(value) = number.as_f64() else {
        return Ok(());
    };
    if schema
        .get("minimum")
        .and_then(Value::as_f64)
        .is_some_and(|minimum| value < minimum)
    {
        return Err(error(path, "is below the declared minimum"));
    }
    if schema
        .get("maximum")
        .and_then(Value::as_f64)
        .is_some_and(|maximum| value > maximum)
    {
        return Err(error(path, "is above the declared maximum"));
    }
    Ok(())
}

fn validate_not(schema: &Value, instance: &Value, path: &str) -> Result<(), SchemaValidationError> {
    let Some(not_schema) = schema.get("not") else {
        return Ok(());
    };
    if validate_at(not_schema, instance, path).is_ok() {
        Err(error(path, "matches a forbidden field combination"))
    } else {
        Ok(())
    }
}

fn validate_one_of(
    schema: &Value,
    instance: &Value,
    path: &str,
) -> Result<(), SchemaValidationError> {
    let Some(branches) = schema.get("oneOf").and_then(Value::as_array) else {
        return Ok(());
    };
    let matches = branches
        .iter()
        .filter(|branch| validate_at(branch, instance, path).is_ok())
        .count();
    if matches == 1 {
        Ok(())
    } else {
        Err(error(path, "must match exactly one declared schema branch"))
    }
}

fn child_path(parent: &str, field: &str) -> String {
    format!("{parent}.{field}")
}

fn error(path: &str, message: impl Into<String>) -> SchemaValidationError {
    SchemaValidationError {
        path: path.to_string(),
        message: message.into(),
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{path} {message}")]
pub(super) struct SchemaValidationError {
    path: String,
    message: String,
}
