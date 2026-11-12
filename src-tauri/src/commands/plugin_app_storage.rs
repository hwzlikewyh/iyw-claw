use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::sync::Mutex;

static STORAGE_LOCK: Mutex<()> = Mutex::const_new(());

pub(super) async fn request(
    workspace: &Path,
    arguments: &Map<String, Value>,
) -> Result<Value, crate::plugin_runtime::types::PluginInvokeError> {
    let operation = string(arguments, "operation")?;
    let namespace = string(arguments, "namespace")?;
    if namespace.len() > 160 || namespace.chars().any(char::is_control) {
        return Err(invalid());
    }
    let _guard = STORAGE_LOCK.lock().await;
    let directory = namespace_dir(workspace, &namespace)?;
    if operation == "list" {
        return list(&directory);
    }
    let key = string(arguments, "key")?;
    if key.len() > 240 || key.chars().any(char::is_control) {
        return Err(invalid());
    }
    let path = directory.join(format!("{}.json", digest(&key)));
    reject_link(&path)?;
    match operation.as_str() {
        "get" => get(&path, &key),
        "set" => set(
            &directory,
            &path,
            &key,
            arguments.get("value").ok_or_else(invalid)?,
        ),
        "delete" => {
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(invalid()),
            }
            Ok(serde_json::json!({ "key": key, "deleted": true }))
        }
        _ => Err(invalid()),
    }
}

fn namespace_dir(
    workspace: &Path,
    namespace: &str,
) -> Result<PathBuf, crate::plugin_runtime::types::PluginInvokeError> {
    let workspace = std::fs::canonicalize(workspace).map_err(|_| invalid())?;
    let mut directory = workspace;
    for part in ["canvas", "infinite-canvas", "app-data"] {
        directory = directory.join(part);
        match std::fs::symlink_metadata(&directory) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(invalid())
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(&directory).map_err(|_| invalid())?;
            }
            Err(_) => return Err(invalid()),
        }
        reject_link(&directory)?;
    }
    directory = directory.join(digest(namespace));
    match std::fs::symlink_metadata(&directory) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(invalid())
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir(&directory).map_err(|_| invalid())?;
        }
        Err(_) => return Err(invalid()),
    }
    reject_link(&directory)?;
    Ok(directory)
}

fn list(directory: &Path) -> Result<Value, crate::plugin_runtime::types::PluginInvokeError> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(directory).map_err(|_| invalid())? {
        let path = entry.map_err(|_| invalid())?.path();
        if path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        reject_link(&path)?;
        let value: Value = serde_json::from_slice(&std::fs::read(&path).map_err(|_| invalid())?)
            .map_err(|_| invalid())?;
        if let (Some(key), Some(value)) = (value["key"].as_str(), value.get("value")) {
            entries.push(serde_json::json!({ "key": key, "value": value }));
        }
    }
    Ok(serde_json::json!({ "entries": entries }))
}

fn get(path: &Path, key: &str) -> Result<Value, crate::plugin_runtime::types::PluginInvokeError> {
    let record: Value = match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| invalid())?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(serde_json::json!({ "value": null }));
        }
        Err(_) => return Err(invalid()),
    };
    if record["key"].as_str() != Some(key) {
        return Err(invalid());
    }
    Ok(serde_json::json!({ "value": record["value"] }))
}

fn set(
    directory: &Path,
    path: &Path,
    key: &str,
    value: &Value,
) -> Result<Value, crate::plugin_runtime::types::PluginInvokeError> {
    let mut temporary = tempfile::NamedTempFile::new_in(directory).map_err(|_| invalid())?;
    serde_json::to_writer(
        temporary.as_file_mut(),
        &serde_json::json!({ "key": key, "value": value }),
    )
    .map_err(|_| invalid())?;
    use std::io::Write;
    temporary.as_file_mut().flush().map_err(|_| invalid())?;
    temporary.as_file().sync_all().map_err(|_| invalid())?;
    temporary.persist(path).map_err(|_| invalid())?;
    Ok(serde_json::json!({ "key": key, "saved": true }))
}

fn reject_link(path: &Path) -> Result<(), crate::plugin_runtime::types::PluginInvokeError> {
    if std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(invalid());
    }
    Ok(())
}

fn string(
    arguments: &Map<String, Value>,
    name: &str,
) -> Result<String, crate::plugin_runtime::types::PluginInvokeError> {
    arguments
        .get(name)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(invalid)
}

fn digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn invalid() -> crate::plugin_runtime::types::PluginInvokeError {
    crate::plugin_runtime::types::PluginInvokeError::before_effect(
        "plugin_app_storage_unavailable",
        "Plugin workspace storage is unavailable",
    )
}
