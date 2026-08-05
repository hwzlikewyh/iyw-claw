use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};

use super::ReconcileError;

const AGENTS_START_MARKER: &str = "<!-- iyw-claw:codex-subagents:start -->";
const AGENTS_END_MARKER: &str = "<!-- iyw-claw:codex-subagents:end -->";
const AGENTS_SECTION: &str = include_str!("../../../resources/codex/agents-section.md");
const DEFAULT_AGENT_TOML: &str = include_str!("../../../resources/codex/agents/default.toml");
const BACKUP_ROOT: &str = "backups/multi-agent-v2";

pub(super) fn reconcile(
    profile_root: &Path,
    config_raw: &str,
    provider_config_next: &str,
) -> Result<bool, ReconcileError> {
    let config_next = patch_config(provider_config_next)?;
    let agents_path = profile_root.join("AGENTS.md");
    let default_path = profile_root.join("agents").join("default.toml");
    let agents_raw = read_optional(&agents_path)?;
    let default_raw = read_optional(&default_path)?;
    let agents_next = patch_agents_md(&agents_raw).map_err(ReconcileError::ParseFailed)?;
    let backup_required = config_raw != config_next.as_str()
        || agents_raw != agents_next
        || default_raw != DEFAULT_AGENT_TOML;

    if backup_required {
        backup_existing_files(profile_root)?;
    }

    let config_path = profile_root.join("config.toml");
    let changed = write(&config_path, config_raw, &config_next)?
        | write(&agents_path, &agents_raw, &agents_next)?
        | write(&default_path, &default_raw, DEFAULT_AGENT_TOML)?;
    verify(profile_root, &config_next)?;
    Ok(changed)
}

fn patch_config(raw: &str) -> Result<String, ReconcileError> {
    let mut value: toml::Value = raw.parse().map_err(|error| {
        ReconcileError::ParseFailed(format!("parse Codex config for V2 migration: {error}"))
    })?;
    let root = value.as_table_mut().ok_or_else(|| {
        ReconcileError::ParseFailed("Codex config root must be a TOML table".into())
    })?;
    crate::acp::codex_multi_agent::patch_toml(root).map_err(ReconcileError::ParseFailed)?;
    toml::to_string_pretty(&value).map_err(|error| ReconcileError::ParseFailed(error.to_string()))
}

fn multi_agent_config_is_current(raw: &str) -> Result<bool, ReconcileError> {
    if raw.trim().is_empty() {
        return Ok(false);
    }
    let value: toml::Value = raw.parse().map_err(|error| {
        ReconcileError::ParseFailed(format!("parse Codex config for migration: {error}"))
    })?;
    Ok(crate::acp::codex_multi_agent::is_current(&value))
}

fn patch_agents_md(raw: &str) -> Result<String, String> {
    let newline = if raw.contains("\r\n") { "\r\n" } else { "\n" };
    let section = managed_agents_section(newline);
    let block = managed_agents_block(newline);
    match managed_marker_range(raw)? {
        Some(range) => Ok(replace_section(raw, range, &block)),
        None => {
            let range = raw
                .find(&section)
                .map(|start| start..start + section.len())
                .unwrap_or(raw.len()..raw.len());
            Ok(replace_section(raw, range, &block))
        }
    }
}

fn managed_marker_range(raw: &str) -> Result<Option<Range<usize>>, String> {
    let starts: Vec<_> = raw.match_indices(AGENTS_START_MARKER).collect();
    let ends: Vec<_> = raw.match_indices(AGENTS_END_MARKER).collect();
    match (starts.as_slice(), ends.as_slice()) {
        ([], []) => Ok(None),
        ([(start, _)], [(end, _)]) if start <= end => {
            Ok(Some(*start..*end + AGENTS_END_MARKER.len()))
        }
        _ => Err("AGENTS.md must contain zero or one matched iyw-claw marker pair".into()),
    }
}

fn managed_agents_section(newline: &str) -> String {
    AGENTS_SECTION
        .trim()
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', newline)
}

fn managed_agents_block(newline: &str) -> String {
    let section = managed_agents_section(newline);
    format!("{AGENTS_START_MARKER}{newline}{section}{newline}{AGENTS_END_MARKER}")
}

fn replace_section(raw: &str, range: Range<usize>, block: &str) -> String {
    let newline = if raw.contains("\r\n") { "\r\n" } else { "\n" };
    let prefix = raw[..range.start].trim_end_matches(['\r', '\n']);
    let suffix = raw[range.end..].trim_start_matches(['\r', '\n']);
    let mut parts = Vec::new();
    if !prefix.is_empty() {
        parts.push(prefix);
    }
    parts.push(block);
    if !suffix.is_empty() {
        parts.push(suffix);
    }
    parts.join(&format!("{newline}{newline}")) + newline
}

fn backup_existing_files(profile_root: &Path) -> Result<(), ReconcileError> {
    let backup_dir = unique_backup_dir(profile_root);
    let relative_paths = ["config.toml", "AGENTS.md", "agents/default.toml"];
    let existing: Vec<&str> = relative_paths
        .into_iter()
        .filter(|relative| profile_root.join(relative).is_file())
        .collect();
    if existing.is_empty() {
        return Ok(());
    }
    let file_count = existing.len();
    for relative in existing {
        let source = profile_root.join(relative);
        let destination = backup_dir.join(relative);
        let parent = destination.parent().ok_or_else(|| {
            ReconcileError::WriteFailed(format!(
                "backup path has no parent: {}",
                destination.display()
            ))
        })?;
        fs::create_dir_all(parent).map_err(|error| backup_error(parent, error))?;
        fs::copy(&source, &destination).map_err(|error| backup_error(&source, error))?;
    }
    tracing::info!(
        file_count,
        "backed up Codex profile before multi-agent V2 migration"
    );
    Ok(())
}

fn unique_backup_dir(profile_root: &Path) -> PathBuf {
    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S%.3f").to_string();
    let suffix = uuid::Uuid::new_v4();
    profile_root
        .join(BACKUP_ROOT)
        .join(format!("{timestamp}-{}", suffix.simple()))
}

fn backup_error(path: &Path, error: impl std::fmt::Display) -> ReconcileError {
    ReconcileError::WriteFailed(format!(
        "Codex profile backup failed at {}: {error}",
        path.display()
    ))
}

fn read_optional(path: &Path) -> Result<String, ReconcileError> {
    super::super::provider_overlay_files::read_optional(path).map_err(ReconcileError::Failed)
}

fn write(path: &Path, raw: &str, next: &str) -> Result<bool, ReconcileError> {
    if raw == next {
        return Ok(false);
    }
    super::super::provider_overlay_files::write_if_changed(path, raw, next)
        .map_err(ReconcileError::WriteFailed)?;
    Ok(true)
}

fn verify(profile_root: &Path, config_expected: &str) -> Result<(), ReconcileError> {
    let config_path = profile_root.join("config.toml");
    let agents_path = profile_root.join("AGENTS.md");
    let default_path = profile_root.join("agents").join("default.toml");
    let config_raw = read_back(&config_path)?;
    if !multi_agent_config_is_current(&config_raw)? {
        return Err(verification_error(
            &config_path,
            "multi-agent V2 settings mismatch",
        ));
    }
    verify_config_values(&config_raw, config_expected, &config_path)?;
    let agents_raw = read_back(&agents_path)?;
    if patch_agents_md(&agents_raw).map_err(ReconcileError::VerificationFailed)? != agents_raw {
        return Err(verification_error(&agents_path, "managed section mismatch"));
    }
    let default_raw = read_back(&default_path)?;
    if default_raw != DEFAULT_AGENT_TOML {
        return Err(verification_error(
            &default_path,
            "managed subagent config mismatch",
        ));
    }
    default_raw.parse::<toml::Value>().map_err(|error| {
        ReconcileError::VerificationFailed(format!("re-parse {}: {error}", default_path.display()))
    })?;
    Ok(())
}

fn verify_config_values(raw: &str, expected: &str, path: &Path) -> Result<(), ReconcileError> {
    let parse = |value: &str| {
        value.parse::<toml::Value>().map_err(|error| {
            ReconcileError::VerificationFailed(format!("re-parse {}: {error}", path.display()))
        })
    };
    let actual = parse(raw)?;
    let expected = parse(expected)?;
    let spec = super::model::codex_spec();
    let actual_fields = super::model::extract_toml_controlled_fields(&actual, &spec);
    let expected_fields = super::model::extract_toml_controlled_fields(&expected, &spec);
    if actual_fields != expected_fields {
        return Err(verification_error(
            path,
            "controlled values changed after write",
        ));
    }
    Ok(())
}

fn read_back(path: &Path) -> Result<String, ReconcileError> {
    fs::read_to_string(path).map_err(|error| {
        ReconcileError::VerificationFailed(format!("read back {}: {error}", path.display()))
    })
}

fn verification_error(path: &Path, message: &str) -> ReconcileError {
    ReconcileError::VerificationFailed(format!("{}: {message}", path.display()))
}
