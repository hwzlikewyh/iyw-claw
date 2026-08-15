use base64::Engine;
use serde::Deserialize;
use serde_json::Value;

use super::error::{BrowserError, BrowserErrorCode, BrowserErrorContext};
use super::types::BrowserGenerations;

const FRAME_MAGIC: &[u8; 4] = b"IYWB";
const FRAME_VERSION: u8 = 1;
const HEADER_LENGTH: u16 = 48;
const MAX_BASE64_FRAME: usize = 12 * 1024 * 1024;
const MAX_VIEWPORT_EDGE: u32 = 16_384;

#[derive(Deserialize)]
struct WireFrame {
    #[serde(default)]
    seq: Option<u64>,
    #[serde(default)]
    data: Option<String>,
    #[serde(default)]
    metadata: WireMetadata,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireMetadata {
    device_width: u32,
    device_height: u32,
}

pub(super) fn encode_frame(
    text: &str,
    generations: &BrowserGenerations,
) -> Result<Option<(u64, Vec<u8>)>, BrowserError> {
    if text.len() > MAX_BASE64_FRAME.saturating_add(4096) {
        return Err(frame_error());
    }
    let value: Value = serde_json::from_str(text).map_err(|_| frame_error())?;
    if value.get("type").and_then(Value::as_str) != Some("frame") {
        return Ok(None);
    }
    let frame: WireFrame = serde_json::from_value(value).map_err(|_| frame_error())?;
    let seq = frame.seq.ok_or_else(frame_error)?;
    let data = frame.data.ok_or_else(frame_error)?;
    if data.len() > MAX_BASE64_FRAME
        || !valid_edge(frame.metadata.device_width)
        || !valid_edge(frame.metadata.device_height)
    {
        return Err(frame_error());
    }
    let jpeg = base64::engine::general_purpose::STANDARD
        .decode(data.as_bytes())
        .map_err(|_| frame_error())?;
    if jpeg.len() < 4 || !jpeg.starts_with(&[0xff, 0xd8]) || !jpeg.ends_with(&[0xff, 0xd9]) {
        return Err(frame_error());
    }
    let mut output = Vec::with_capacity(usize::from(HEADER_LENGTH) + jpeg.len());
    output.extend_from_slice(FRAME_MAGIC);
    output.push(FRAME_VERSION);
    output.push(0);
    output.extend_from_slice(&HEADER_LENGTH.to_le_bytes());
    output.extend_from_slice(&generations.runtime_generation.to_le_bytes());
    output.extend_from_slice(&generations.tab_generation.to_le_bytes());
    output.extend_from_slice(&generations.view_generation.to_le_bytes());
    output.extend_from_slice(&seq.to_le_bytes());
    output.extend_from_slice(&frame.metadata.device_width.to_le_bytes());
    output.extend_from_slice(&frame.metadata.device_height.to_le_bytes());
    output.extend_from_slice(&jpeg);
    Ok(Some((seq, output)))
}

pub(super) fn frame_sequence(text: &str) -> Option<u64> {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()?
        .get("seq")?
        .as_u64()
}

fn valid_edge(value: u32) -> bool {
    value > 0 && value <= MAX_VIEWPORT_EDGE
}

fn frame_error() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserFrameDecodeFailed,
        "The browser frame could not be decoded",
    )
}

pub(super) fn ensure_frame_generations(
    actual: &BrowserGenerations,
    expected: &BrowserGenerations,
) -> Result<(), BrowserError> {
    if actual.runtime_generation == expected.runtime_generation
        && actual.tab_generation == expected.tab_generation
        && actual.view_generation == expected.view_generation
    {
        return Ok(());
    }
    Err(BrowserError::stale_generation(BrowserErrorContext {
        runtime_generation: Some(expected.runtime_generation),
        tab_generation: Some(expected.tab_generation),
        view_generation: Some(expected.view_generation),
        control_epoch: Some(expected.control_epoch),
        ..BrowserErrorContext::default()
    }))
}
