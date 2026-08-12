use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::{json, Value};
use walkdir::WalkDir;

const COMPACTION_OBSERVE_ATTEMPTS: usize = 20;
const COMPACTION_OBSERVE_INTERVAL_MS: u64 = 100;

struct LegacyCall {
    caption: Option<String>,
    name: Option<String>,
    source_kind: Option<String>,
}

struct LegacyImage {
    call_id: String,
    bytes: Vec<u8>,
    mime_type: &'static str,
    caption: Option<String>,
    name: String,
    source_kind: Option<String>,
}

pub(super) struct CompactionMarker {
    paths: Vec<PathBuf>,
    count: usize,
}

pub(super) struct MigratedImage {
    pub metadata: Value,
    pub metadata_text: String,
    pub arguments: Value,
}

pub async fn migrate_resumed_session(session_id: &str) -> Result<usize, String> {
    let paths = find_rollouts(session_id)?;
    let mut migrated = 0;
    for path in paths {
        migrated += migrate_rollout(path).await?;
    }
    Ok(migrated)
}

pub(super) async fn compaction_marker(session_id: &str) -> Result<CompactionMarker, String> {
    let session_id = session_id.to_string();
    tokio::task::spawn_blocking(move || {
        let paths = find_rollouts(&session_id)?;
        let count = count_compacted_records(&paths)?;
        Ok(CompactionMarker { paths, count })
    })
    .await
    .map_err(|error| format!("Codex compaction marker task failed: {error}"))?
}

pub(super) async fn wait_for_new_compaction(marker: &CompactionMarker) -> Result<bool, String> {
    for _ in 0..COMPACTION_OBSERVE_ATTEMPTS {
        let paths = marker.paths.clone();
        let count = tokio::task::spawn_blocking(move || count_compacted_records(&paths))
            .await
            .map_err(|error| format!("Codex compaction check task failed: {error}"))??;
        if count > marker.count {
            return Ok(true);
        }
        tokio::time::sleep(std::time::Duration::from_millis(
            COMPACTION_OBSERVE_INTERVAL_MS,
        ))
        .await;
    }
    Ok(false)
}

fn count_compacted_records(paths: &[PathBuf]) -> Result<usize, String> {
    let mut count = 0;
    for path in paths {
        let reader = BufReader::new(File::open(path).map_err(file_error)?);
        for line in reader.lines() {
            let line = line.map_err(file_error)?;
            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if value.get("type").and_then(Value::as_str) == Some("compacted") {
                count += 1;
            }
        }
    }
    Ok(count)
}

async fn migrate_rollout(path: PathBuf) -> Result<usize, String> {
    let scan_path = path.clone();
    let images = tokio::task::spawn_blocking(move || scan_rollout(&scan_path))
        .await
        .map_err(|error| format!("rollout scan task failed: {error}"))??;
    if images.is_empty() {
        return Ok(0);
    }
    let mut migrated = HashMap::new();
    for mut image in images {
        let bytes = std::mem::take(&mut image.bytes);
        let asset = crate::display_assets::store(bytes, image.mime_type).await?;
        let entry = migrated_image(image, asset.uri, asset.mime_type);
        migrated.insert(entry.0, entry.1);
    }
    let count = migrated.len();
    tokio::task::spawn_blocking(move || {
        super::codex_rollout_migration_io::rewrite_rollout(&path, &migrated)
    })
    .await
    .map_err(|error| format!("rollout rewrite task failed: {error}"))??;
    Ok(count)
}

fn find_rollouts(session_id: &str) -> Result<Vec<PathBuf>, String> {
    if session_id.is_empty()
        || session_id.len() > 128
        || !session_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("invalid Codex session identifier".to_string());
    }
    let sessions = crate::parsers::codex::resolve_codex_home_dir().join("sessions");
    if !sessions.is_dir() {
        return Ok(Vec::new());
    }
    let mut paths = WalkDir::new(sessions)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| is_matching_rollout(path, session_id))
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

fn is_matching_rollout(path: &Path, session_id: &str) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    name.starts_with("rollout-") && name.ends_with(".jsonl") && name.contains(session_id)
}

fn scan_rollout(path: &Path) -> Result<Vec<LegacyImage>, String> {
    let reader = BufReader::new(File::open(path).map_err(file_error)?);
    let mut calls = HashMap::<String, LegacyCall>::new();
    let mut images = HashMap::<String, LegacyImage>::new();
    for line in reader.lines() {
        let line = line.map_err(file_error)?;
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let Some(payload) = response_payload(&value) else {
            continue;
        };
        match payload.get("type").and_then(Value::as_str) {
            Some("function_call")
                if payload.get("name").and_then(Value::as_str) == Some("show_image") =>
            {
                if let Some((call_id, call)) = parse_legacy_call(payload) {
                    calls.insert(call_id, call);
                }
            }
            Some("function_call_output") => collect_legacy_image(payload, &calls, &mut images)?,
            _ => {}
        }
    }
    Ok(images.into_values().collect())
}

fn response_payload(value: &Value) -> Option<&Value> {
    (value.get("type").and_then(Value::as_str) == Some("response_item"))
        .then(|| value.get("payload"))
        .flatten()
}

fn parse_legacy_call(payload: &Value) -> Option<(String, LegacyCall)> {
    let call_id = payload.get("call_id")?.as_str()?.to_string();
    let arguments = payload.get("arguments")?;
    let parsed = if let Some(raw) = arguments.as_str() {
        serde_json::from_str::<Value>(raw).ok()?
    } else {
        arguments.clone()
    };
    let source = parsed.get("source").and_then(Value::as_str);
    let source_kind = source.map(|value| {
        if value.starts_with("http://") || value.starts_with("https://") {
            "url".to_string()
        } else {
            "file".to_string()
        }
    });
    Some((
        call_id,
        LegacyCall {
            caption: string_field(&parsed, "caption"),
            name: string_field(&parsed, "name"),
            source_kind,
        },
    ))
}

fn collect_legacy_image(
    payload: &Value,
    calls: &HashMap<String, LegacyCall>,
    images: &mut HashMap<String, LegacyImage>,
) -> Result<(), String> {
    let Some(call_id) = payload.get("call_id").and_then(Value::as_str) else {
        return Ok(());
    };
    let Some(call) = calls.get(call_id) else {
        return Ok(());
    };
    let Some(output) = payload.get("output").and_then(Value::as_array) else {
        return Ok(());
    };
    let Some(image) = parse_output_image(call_id, call, output)? else {
        return Ok(());
    };
    if images.insert(call_id.to_string(), image).is_some() {
        return Err(format!(
            "multiple legacy show_image outputs for call {call_id}"
        ));
    }
    Ok(())
}

fn parse_output_image(
    call_id: &str,
    call: &LegacyCall,
    output: &[Value],
) -> Result<Option<LegacyImage>, String> {
    let image_items = output
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("input_image"))
        .collect::<Vec<_>>();
    if image_items.is_empty() {
        return Ok(None);
    }
    if image_items.len() != 1 {
        return Err(format!(
            "ambiguous legacy show_image output for call {call_id}"
        ));
    }
    let metadata = legacy_metadata(output);
    let image_url = image_items[0]
        .get("image_url")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing legacy image data for call {call_id}"))?;
    let (mime_type, bytes) = decode_data_uri(image_url)?;
    let name = metadata
        .as_ref()
        .and_then(|value| string_field(value, "name"))
        .or_else(|| call.name.clone())
        .unwrap_or_else(|| crate::display_assets::default_name(mime_type));
    Ok(Some(LegacyImage {
        call_id: call_id.to_string(),
        bytes,
        mime_type,
        caption: metadata
            .as_ref()
            .and_then(|value| string_field(value, "caption"))
            .or_else(|| call.caption.clone()),
        name,
        source_kind: metadata
            .as_ref()
            .and_then(|value| string_field(value, "source_kind"))
            .filter(|value| value == "file" || value == "url")
            .or_else(|| call.source_kind.clone()),
    }))
}

fn legacy_metadata(output: &[Value]) -> Option<Value> {
    output.iter().find_map(|item| {
        let text = item.get("text").and_then(Value::as_str)?;
        let value = serde_json::from_str::<Value>(text).ok()?;
        (value.get("type").and_then(Value::as_str) == Some("iyw_claw_display_image")
            && value.get("uri").is_none())
        .then_some(value)
    })
}

fn decode_data_uri(value: &str) -> Result<(&'static str, Vec<u8>), String> {
    let (header, encoded) = value
        .split_once(',')
        .ok_or_else(|| "invalid legacy image Data URI".to_string())?;
    let declared = header
        .strip_prefix("data:")
        .and_then(|value| value.strip_suffix(";base64"))
        .and_then(crate::acp::delegation::image_format::normalize_mime)
        .ok_or_else(|| "unsupported legacy image format".to_string())?;
    let max = crate::acp::delegation::image_loader::MAX_IMAGE_BYTES;
    if encoded.len() > max.div_ceil(3) * 4 + 2 {
        return Err("legacy display image exceeds the size limit".to_string());
    }
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|error| format!("invalid legacy image Base64: {error}"))?;
    if bytes.is_empty() || bytes.len() > max {
        return Err("legacy display image size is invalid".to_string());
    }
    if crate::acp::delegation::image_format::detect_mime(&bytes) != Some(declared) {
        return Err("legacy image content does not match its format".to_string());
    }
    Ok((declared, bytes))
}

fn migrated_image(
    image: LegacyImage,
    uri: String,
    mime_type: &'static str,
) -> (String, MigratedImage) {
    let metadata = json!({
        "type": "iyw_claw_display_image",
        "caption": image.caption,
        "name": image.name,
        "source_kind": image.source_kind,
        "uri": uri,
        "mime_type": mime_type,
    });
    let arguments = json!({ "caption": metadata["caption"], "name": metadata["name"] });
    let metadata_text = metadata.to_string();
    (
        image.call_id,
        MigratedImage {
            metadata,
            metadata_text,
            arguments,
        },
    )
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn file_error(error: impl std::fmt::Display) -> String {
    format!("Codex rollout migration failed: {error}")
}
