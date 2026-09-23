use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::app_error::AppCommandError;

pub(super) fn relocate(
    staging: &Path,
    mappings: &[(String, PathBuf)],
) -> Result<(), AppCommandError> {
    if mappings.is_empty() {
        return Ok(());
    }
    for name in ["external", "acp-transcripts"] {
        let root = staging.join(name);
        if !root.is_dir() {
            continue;
        }
        for entry in walkdir::WalkDir::new(root).follow_links(false) {
            let entry = entry.map_err(|error| AppCommandError::io_error(error.to_string()))?;
            if entry.file_type().is_file() {
                relocate_file(entry.path(), mappings)?;
            }
        }
    }
    Ok(())
}

fn relocate_file(path: &Path, mappings: &[(String, PathBuf)]) -> Result<(), AppCommandError> {
    if path.file_name().is_some_and(|name| name == ".project_root") {
        let value = std::fs::read_to_string(path).map_err(AppCommandError::io)?;
        if let Some(next) = super::portable::rebase(value.trim(), mappings) {
            std::fs::write(path, next).map_err(AppCommandError::io)?;
        }
        return Ok(());
    }
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("jsonl") => relocate_jsonl(path, mappings),
        Some("json") => relocate_json(path, mappings),
        _ => Ok(()),
    }
}

fn relocate_json(path: &Path, mappings: &[(String, PathBuf)]) -> Result<(), AppCommandError> {
    let bytes = std::fs::read(path).map_err(AppCommandError::io)?;
    // 原生日志中可能有非 JSON 文件；不因迁移重写其内容。
    let Ok(mut value) = serde_json::from_slice::<Value>(&bytes) else {
        return Ok(());
    };
    let projects_changed = relocate_project_keys(&mut value, mappings)?;
    if relocate_value(&mut value, mappings) || projects_changed {
        let bytes = serde_json::to_vec(&value).map_err(json_error)?;
        std::fs::write(path, bytes).map_err(AppCommandError::io)?;
    }
    Ok(())
}

fn relocate_project_keys(
    value: &mut Value,
    mappings: &[(String, PathBuf)],
) -> Result<bool, AppCommandError> {
    let Some(projects) = value.get_mut("projects").and_then(Value::as_object_mut) else {
        return Ok(false);
    };
    let mut changed = false;
    for key in projects.keys().cloned().collect::<Vec<_>>() {
        let Some(next) = super::portable::rebase(&key, mappings).filter(|next| next != &key) else {
            continue;
        };
        if projects.contains_key(&next) {
            return Err(AppCommandError::invalid_input(
                "Restored project paths conflict",
            ));
        }
        if let Some(alias) = projects.remove(&key) {
            projects.insert(next, alias);
            changed = true;
        }
    }
    Ok(changed)
}

fn relocate_jsonl(path: &Path, mappings: &[(String, PathBuf)]) -> Result<(), AppCommandError> {
    let input = BufReader::new(File::open(path).map_err(AppCommandError::io)?);
    let mut temp = tempfile::NamedTempFile::new_in(path.parent().unwrap_or(Path::new(".")))
        .map_err(AppCommandError::io)?;
    let mut writer = BufWriter::new(temp.as_file_mut());
    let mut changed = false;
    for line in input.split(b'\n') {
        let line = line.map_err(AppCommandError::io)?;
        let next = match serde_json::from_slice::<Value>(&line) {
            Ok(mut value) => match relocate_value(&mut value, mappings) {
                true => {
                    changed = true;
                    serde_json::to_vec(&value).map_err(json_error)?
                }
                false => line,
            },
            _ => line,
        };
        writer.write_all(&next).map_err(AppCommandError::io)?;
        writer.write_all(b"\n").map_err(AppCommandError::io)?;
    }
    writer.flush().map_err(AppCommandError::io)?;
    drop(writer);
    if changed {
        temp.persist(path)
            .map_err(|error| AppCommandError::io(error.error))?;
    }
    Ok(())
}

pub(super) fn relocate_value(value: &mut Value, mappings: &[(String, PathBuf)]) -> bool {
    match value {
        Value::Array(values) => values.iter_mut().fold(false, |changed, value| {
            relocate_value(value, mappings) || changed
        }),
        Value::Object(values) => values.iter_mut().fold(false, |changed, (key, value)| {
            if is_path_field(key) && relocate_path_value(value, mappings) {
                return true;
            }
            relocate_value(value, mappings) || changed
        }),
        _ => false,
    }
}

fn is_path_field(key: &str) -> bool {
    matches!(
        key,
        "cwd"
            | "directory"
            | "working_directory"
            | "workingDirectory"
            | "workDir"
            | "sessionDir"
            | "path"
            | "file_path"
            | "filePath"
            | "source_path"
            | "sourcePath"
            | "url"
            | "uri"
            | "image_url"
            | "imageUrl"
            | "rollout_path"
            | "rolloutPath"
            | "projectPath"
            | "project_path"
            | "worktree"
    )
}

fn relocate_path_value(value: &mut Value, mappings: &[(String, PathBuf)]) -> bool {
    let Value::String(text) = value else {
        return false;
    };
    let Some(next) =
        super::portable::rebase(text, mappings).filter(|next| next.as_str() != text.as_str())
    else {
        return false;
    };
    *text = next;
    true
}

fn json_error(error: serde_json::Error) -> AppCommandError {
    AppCommandError::invalid_input("Relocate transcript paths").with_detail(error.to_string())
}
