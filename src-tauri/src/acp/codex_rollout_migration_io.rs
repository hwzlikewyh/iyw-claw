use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use serde_json::{json, Value};

use super::codex_rollout_migration::MigratedImage;

const BACKUP_SUFFIX: &str = ".before-display-assets.bak";

pub(super) fn rewrite_rollout(
    path: &Path,
    migrated: &HashMap<String, MigratedImage>,
) -> Result<(), String> {
    write_backup_once(path)?;
    let parent = path
        .parent()
        .ok_or_else(|| "rollout has no parent".to_string())?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(file_error)?;
    let mut reader = BufReader::new(File::open(path).map_err(file_error)?);
    let mut line = Vec::new();
    while reader.read_until(b'\n', &mut line).map_err(file_error)? != 0 {
        write_migrated_line(&line, migrated, &mut temp)?;
        line.clear();
    }
    temp.as_file().sync_all().map_err(file_error)?;
    let temp_path = temp.into_temp_path();
    replace_file(&temp_path, path)
}

fn write_migrated_line(
    line: &[u8],
    migrated: &HashMap<String, MigratedImage>,
    writer: &mut impl Write,
) -> Result<(), String> {
    let content = line.strip_suffix(b"\n").unwrap_or(line);
    let content = content.strip_suffix(b"\r").unwrap_or(content);
    let Ok(mut value) = serde_json::from_slice::<Value>(content) else {
        return writer.write_all(line).map_err(file_error);
    };
    if !migrate_value(&mut value, migrated) {
        return writer.write_all(line).map_err(file_error);
    }
    serde_json::to_writer(&mut *writer, &value).map_err(file_error)?;
    let newline: &[u8] = if line.ends_with(b"\r\n") {
        b"\r\n"
    } else {
        b"\n"
    };
    writer.write_all(newline).map_err(file_error)
}

fn migrate_value(value: &mut Value, migrated: &HashMap<String, MigratedImage>) -> bool {
    let Some(payload) = value.get_mut("payload") else {
        return false;
    };
    let Some(call_id) = payload.get("call_id").and_then(Value::as_str) else {
        return false;
    };
    let Some(image) = migrated.get(call_id) else {
        return false;
    };
    match payload.get("type").and_then(Value::as_str) {
        Some("function_call")
            if payload.get("name").and_then(Value::as_str) == Some("show_image") =>
        {
            payload["arguments"] = Value::String(image.arguments.to_string());
            true
        }
        Some("function_call_output") if output_has_image(payload) => {
            payload["output"] = json!([{ "type": "input_text", "text": image.metadata_text }]);
            true
        }
        Some("mcp_tool_call_end") => migrate_event(payload, image),
        _ => false,
    }
}

fn output_has_image(payload: &Value) -> bool {
    payload
        .get("output")
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items
                .iter()
                .any(|item| item.get("type").and_then(Value::as_str) == Some("input_image"))
        })
}

fn migrate_event(payload: &mut Value, image: &MigratedImage) -> bool {
    if let Some(invocation) = payload.get_mut("invocation") {
        invocation["arguments"] = image.arguments.clone();
    }
    let tool_result = json!({
        "content": [{ "type": "text", "text": image.metadata_text }],
        "isError": false,
        "structuredContent": image.metadata,
    });
    payload["result"] = json!({
        "Ok": {
            "content": [{ "type": "text", "text": tool_result.to_string() }],
            "isError": false,
        }
    });
    true
}

fn write_backup_once(path: &Path) -> Result<(), String> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("rollout");
    let backup = path.with_file_name(format!("{name}{BACKUP_SUFFIX}"));
    if backup.exists() {
        return Ok(());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "rollout has no parent".to_string())?;
    let mut source = File::open(path).map_err(file_error)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(file_error)?;
    std::io::copy(&mut source, &mut temp).map_err(file_error)?;
    temp.as_file().sync_all().map_err(file_error)?;
    match temp.persist_noclobber(backup) {
        Ok(_) => Ok(()),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(file_error(error.error)),
    }
}

#[cfg(unix)]
fn replace_file(temp: &Path, target: &Path) -> Result<(), String> {
    std::fs::rename(temp, target).map_err(file_error)
}

#[cfg(windows)]
fn replace_file(temp: &Path, target: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let wide = |path: &Path| {
        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>()
    };
    let source = wide(temp);
    let destination = wide(target);
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        return Err(file_error(std::io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn replace_file(temp: &Path, target: &Path) -> Result<(), String> {
    std::fs::rename(temp, target).map_err(file_error)
}

fn file_error(error: impl std::fmt::Display) -> String {
    format!("Codex rollout migration failed: {error}")
}
