use std::fs;
use std::io::Read;
use std::path::Path;

/// Read compressed logs when present, otherwise support uncompressed JSONL.
pub(super) fn read_session_log_text(session_dir: &Path) -> Option<String> {
    let zstd_path = session_dir.join("session.jsonl.zstd");
    match fs::read(&zstd_path) {
        Ok(bytes) => decode_zstd_frames_prefix(&bytes)
            .or_else(|| fs::read_to_string(session_dir.join("session.jsonl")).ok()),
        Err(_) => fs::read_to_string(session_dir.join("session.jsonl")).ok(),
    }
}

/// Preserve decoded frames before a concurrently written, incomplete zstd tail.
fn decode_zstd_frames_prefix(bytes: &[u8]) -> Option<String> {
    let mut decoder = zstd::stream::read::Decoder::with_buffer(bytes).ok()?;
    let mut decoded = Vec::new();
    match decoder.read_to_end(&mut decoded) {
        Ok(_) => Some(String::from_utf8_lossy(&decoded).into_owned()),
        Err(_) if !decoded.is_empty() => Some(String::from_utf8_lossy(&decoded).into_owned()),
        Err(_) => None,
    }
}
