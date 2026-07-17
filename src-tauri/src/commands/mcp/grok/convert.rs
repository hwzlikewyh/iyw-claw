use serde_json::{Map, Value};

use crate::app_error::AppCommandError;

use super::super::{canonicalize_spec, json_to_toml_value, mcp_invalid_input, toml_to_json_value};

const TRANSPORT_KEYS: &[&str] = &["type", "command", "args", "env", "cwd", "url", "headers"];

pub(super) fn canonical_to_entry(spec: &Value) -> Result<toml::Value, AppCommandError> {
    let canonical = canonicalize_spec(spec, "Grok conversion")?;
    let object = canonical
        .as_object()
        .ok_or_else(|| mcp_invalid_input("Grok conversion: spec must be an object"))?;
    let transport = object
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("stdio");
    let mut table = match transport {
        "stdio" => stdio_to_table(object)?,
        "http" | "sse" => remote_to_table(object, transport)?,
        other => {
            return Err(mcp_invalid_input(format!(
                "Grok conversion: unsupported MCP type '{other}'"
            )))
        }
    };
    copy_json_extras(object, &mut table);
    Ok(toml::Value::Table(table))
}

fn stdio_to_table(
    object: &Map<String, Value>,
) -> Result<toml::map::Map<String, toml::Value>, AppCommandError> {
    let command = object
        .get("command")
        .and_then(Value::as_str)
        .ok_or_else(|| mcp_invalid_input("Grok stdio server is missing command"))?;
    let mut table = toml::map::Map::new();
    table.insert("command".into(), toml::Value::String(command.into()));
    insert_string_array(&mut table, "args", object.get("args"));
    insert_string_map(&mut table, "env", object.get("env"));
    insert_optional_string(&mut table, "cwd", object.get("cwd"));
    Ok(table)
}

fn remote_to_table(
    object: &Map<String, Value>,
    transport: &str,
) -> Result<toml::map::Map<String, toml::Value>, AppCommandError> {
    let url = object
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| mcp_invalid_input("Grok remote server is missing url"))?;
    let mut table = toml::map::Map::new();
    if transport == "sse" {
        table.insert("type".into(), toml::Value::String("sse".into()));
    }
    table.insert("url".into(), toml::Value::String(url.into()));
    insert_string_map(&mut table, "headers", object.get("headers"));
    Ok(table)
}

pub(super) fn entry_to_canonical(id: &str, value: &toml::Value) -> Result<Value, AppCommandError> {
    let table = value
        .as_table()
        .ok_or_else(|| mcp_invalid_input(format!("Grok MCP entry '{id}' must be a table")))?;
    let mut spec = Map::new();
    if is_remote(table) {
        read_remote(table, &mut spec);
    } else {
        read_stdio(table, &mut spec);
    }
    copy_toml_extras(table, &mut spec);
    canonicalize_spec(&Value::Object(spec), "Grok config")
}

fn is_remote(table: &toml::map::Map<String, toml::Value>) -> bool {
    let explicit = table
        .get("type")
        .and_then(toml::Value::as_str)
        .map(str::trim);
    let has_url = table
        .get("url")
        .and_then(toml::Value::as_str)
        .is_some_and(|url| !url.trim().is_empty());
    matches!(explicit, Some("http") | Some("sse")) || (has_url && explicit != Some("stdio"))
}

fn read_remote(table: &toml::map::Map<String, toml::Value>, spec: &mut Map<String, Value>) {
    let transport = if table.get("type").and_then(toml::Value::as_str) == Some("sse") {
        "sse"
    } else {
        "http"
    };
    spec.insert("type".into(), Value::String(transport.into()));
    copy_toml_string(table, spec, "url");
    copy_toml_string_map(table, spec, "headers");
}

fn read_stdio(table: &toml::map::Map<String, toml::Value>, spec: &mut Map<String, Value>) {
    spec.insert("type".into(), Value::String("stdio".into()));
    copy_toml_string(table, spec, "command");
    let args = table
        .get("args")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| Value::String(value.into()))
        .collect::<Vec<_>>();
    if !args.is_empty() {
        spec.insert("args".into(), Value::Array(args));
    }
    copy_toml_string_map(table, spec, "env");
    copy_toml_string(table, spec, "cwd");
}

fn insert_optional_string(
    table: &mut toml::map::Map<String, toml::Value>,
    key: &str,
    value: Option<&Value>,
) {
    if let Some(value) = value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        table.insert(key.into(), toml::Value::String(value.into()));
    }
}

fn insert_string_array(
    table: &mut toml::map::Map<String, toml::Value>,
    key: &str,
    value: Option<&Value>,
) {
    let values = value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| toml::Value::String(value.into()))
        .collect::<Vec<_>>();
    if !values.is_empty() {
        table.insert(key.into(), toml::Value::Array(values));
    }
}

fn insert_string_map(
    table: &mut toml::map::Map<String, toml::Value>,
    key: &str,
    value: Option<&Value>,
) {
    let Some(object) = value.and_then(Value::as_object) else {
        return;
    };
    let values = object
        .iter()
        .filter_map(|(key, value)| {
            value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| (key.clone(), toml::Value::String(value.into())))
        })
        .collect::<toml::map::Map<_, _>>();
    if !values.is_empty() {
        table.insert(key.into(), toml::Value::Table(values));
    }
}

fn copy_json_extras(object: &Map<String, Value>, table: &mut toml::map::Map<String, toml::Value>) {
    for (key, value) in object {
        if !TRANSPORT_KEYS.contains(&key.as_str()) {
            if let Some(value) = json_to_toml_value(value) {
                table.insert(key.clone(), value);
            }
        }
    }
}

fn copy_toml_extras(table: &toml::map::Map<String, toml::Value>, spec: &mut Map<String, Value>) {
    for (key, value) in table {
        if !TRANSPORT_KEYS.contains(&key.as_str()) {
            spec.insert(key.clone(), toml_to_json_value(value));
        }
    }
}

fn copy_toml_string(
    table: &toml::map::Map<String, toml::Value>,
    spec: &mut Map<String, Value>,
    key: &str,
) {
    if let Some(value) = table
        .get(key)
        .and_then(toml::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        spec.insert(key.into(), Value::String(value.into()));
    }
}

fn copy_toml_string_map(
    table: &toml::map::Map<String, toml::Value>,
    spec: &mut Map<String, Value>,
    key: &str,
) {
    let Some(values) = table.get(key).and_then(toml::Value::as_table) else {
        return;
    };
    let mapped = values
        .iter()
        .filter_map(|(key, value)| {
            value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| (key.clone(), Value::String(value.into())))
        })
        .collect::<Map<_, _>>();
    if !mapped.is_empty() {
        spec.insert(key.into(), Value::Object(mapped));
    }
}
