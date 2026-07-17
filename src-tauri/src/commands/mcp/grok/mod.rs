mod convert;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::app_error::AppCommandError;

use super::mcp_configuration_invalid;
use convert::{canonical_to_entry, entry_to_canonical};

fn config_path() -> PathBuf {
    crate::parsers::grok::resolve_grok_home_dir().join("config.toml")
}

fn read_root(path: &Path) -> Result<toml::Value, AppCommandError> {
    if !path.exists() {
        return Ok(toml::Value::Table(toml::map::Map::new()));
    }
    let raw = fs::read_to_string(path).map_err(AppCommandError::io)?;
    let root = raw.parse::<toml::Value>().map_err(|error| {
        mcp_configuration_invalid(format!("invalid TOML at {}: {error}", path.display()))
    })?;
    if root.is_table() {
        Ok(root)
    } else {
        Err(mcp_configuration_invalid(format!(
            "invalid TOML root at {}: expected table",
            path.display()
        )))
    }
}

fn write_root(path: &Path, root: &toml::Value) -> Result<(), AppCommandError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(AppCommandError::io)?;
    }
    let serialized = toml::to_string_pretty(root).map_err(|error| {
        mcp_configuration_invalid(format!(
            "failed to serialize TOML for {}: {error}",
            path.display()
        ))
    })?;
    fs::write(path, format!("{serialized}\n")).map_err(AppCommandError::io)
}

pub(super) fn read_servers() -> Result<BTreeMap<String, Value>, AppCommandError> {
    read_servers_at(&config_path())
}

pub(super) fn read_servers_at(path: &Path) -> Result<BTreeMap<String, Value>, AppCommandError> {
    let root = read_root(path)?;
    let mut result = BTreeMap::new();
    let Some(servers) = root
        .as_table()
        .and_then(|table| table.get("mcp_servers"))
        .and_then(toml::Value::as_table)
    else {
        return Ok(result);
    };
    for (id, entry) in servers {
        match entry_to_canonical(id, entry) {
            Ok(spec) => {
                result.insert(id.clone(), spec);
            }
            Err(error) => tracing::warn!("[MCP] skip invalid Grok server {id}: {error}"),
        }
    }
    Ok(result)
}

pub(super) fn upsert_server(id: &str, spec: &Value) -> Result<(), AppCommandError> {
    upsert_server_at(&config_path(), id, spec)
}

pub(super) fn upsert_server_at(path: &Path, id: &str, spec: &Value) -> Result<(), AppCommandError> {
    let mut root = read_root(path)?;
    let root_table = root
        .as_table_mut()
        .ok_or_else(|| mcp_configuration_invalid("Grok config root must be a TOML table"))?;
    let servers = ensure_servers_table(root_table)?;
    servers.insert(id.to_string(), canonical_to_entry(spec)?);
    write_root(path, &root)
}

pub(super) fn remove_server(id: &str) -> Result<bool, AppCommandError> {
    remove_server_at(&config_path(), id)
}

pub(super) fn remove_server_at(path: &Path, id: &str) -> Result<bool, AppCommandError> {
    if !path.exists() {
        return Ok(false);
    }
    let mut root = read_root(path)?;
    let Some(root_table) = root.as_table_mut() else {
        return Ok(false);
    };
    let removed = remove_from_servers(root_table, id);
    if removed {
        write_root(path, &root)?;
    }
    Ok(removed)
}

fn ensure_servers_table(
    root: &mut toml::map::Map<String, toml::Value>,
) -> Result<&mut toml::map::Map<String, toml::Value>, AppCommandError> {
    let servers = root
        .entry("mcp_servers")
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    servers
        .as_table_mut()
        .ok_or_else(|| mcp_configuration_invalid("Grok mcp_servers must be a TOML table"))
}

fn remove_from_servers(root: &mut toml::map::Map<String, toml::Value>, id: &str) -> bool {
    let Some(servers) = root
        .get_mut("mcp_servers")
        .and_then(toml::Value::as_table_mut)
    else {
        return false;
    };
    let removed = servers.remove(id).is_some();
    if servers.is_empty() {
        root.remove("mcp_servers");
    }
    removed
}
