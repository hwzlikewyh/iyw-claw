use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};

use base64::engine::general_purpose::STANDARD;
use base64::read::DecoderReader;
use sha2::{Digest, Sha256};

use super::{
    error::GatewayError,
    json_scan::ImageSlot,
    limits::{IO_BUFFER_BYTES, MAX_IMAGE_BYTES},
    spool::Spool,
};

pub(super) struct Prepared {
    pub file: std::fs::File,
    pub digest: String,
    pub encoded_digest: [u8; 32],
    pub mime: &'static str,
    pub size: u64,
}

pub(super) fn encoded_digest(body: &Spool, slot: &ImageSlot) -> Result<[u8; 32], GatewayError> {
    let reader = body.reader_at(slot.encoded.start)?;
    let mut hash = Sha256::new();
    let mut bytes = [0_u8; IO_BUFFER_BYTES];
    let mut reader = reader.take(slot.encoded.end - slot.encoded.start);
    loop {
        let size = reader.read(&mut bytes)?;
        if size == 0 {
            break;
        }
        hash.update(&bytes[..size]);
    }
    Ok(hash.finalize().into())
}

pub(super) fn decode(
    body: &Spool,
    slot: &ImageSlot,
    encoded_digest: [u8; 32],
) -> Result<Prepared, GatewayError> {
    let reader = body.reader_at(slot.encoded.start + 1)?;
    let raw = reader.take(slot.encoded.end - slot.encoded.start - 2);
    let mut reader = BufReader::with_capacity(IO_BUFFER_BYTES, JsonStringReader(raw));
    let prefix = reader.fill_buf()?;
    let mut declared_mime = slot.mime.clone();
    if prefix.starts_with(b"data:") {
        let end = prefix
            .iter()
            .position(|byte| *byte == b',')
            .ok_or_else(|| GatewayError::input("Image data URL header is invalid"))?;
        if end > 128 || !prefix[..end].ends_with(b";base64") {
            return Err(GatewayError::input("Image data URL must use Base64"));
        }
        let mime = std::str::from_utf8(&prefix[5..end - 7])
            .map_err(|_| GatewayError::input("Image data URL MIME is invalid"))?;
        if declared_mime
            .as_deref()
            .is_some_and(|declared| declared != mime)
        {
            return Err(GatewayError::input("Image declared MIME types conflict"));
        }
        declared_mime = Some(mime.to_owned());
        reader.consume(end + 1);
    }
    let (mut file, hash, signature, size) = decode_to_file(reader)?;
    let mime = detect_mime(&signature)
        .ok_or_else(|| GatewayError::input("Image format is unsupported"))?;
    if declared_mime
        .as_deref()
        .is_some_and(|declared| declared != mime)
    {
        return Err(GatewayError::input(
            "Image MIME type does not match its bytes",
        ));
    }
    file.flush()?;
    file.seek(SeekFrom::Start(0))?;
    Ok(Prepared {
        file,
        digest: format!("{:x}", hash.finalize()),
        encoded_digest,
        mime,
        size,
    })
}

fn decode_to_file(
    reader: impl Read,
) -> Result<(std::fs::File, Sha256, Vec<u8>, u64), GatewayError> {
    let mut decoded = DecoderReader::new(reader, &STANDARD);
    let mut file = tempfile::tempfile()?;
    let mut hash = Sha256::new();
    let mut size = 0_u64;
    let mut bytes = [0_u8; IO_BUFFER_BYTES];
    let mut signature = Vec::with_capacity(12);
    loop {
        let count = decoded
            .read(&mut bytes)
            .map_err(|_| GatewayError::input("Image Base64 is invalid"))?;
        if count == 0 {
            break;
        }
        size += count as u64;
        if size > MAX_IMAGE_BYTES {
            return Err(GatewayError::input("Image exceeds 32 MiB"));
        }
        let needed = 12_usize.saturating_sub(signature.len()).min(count);
        signature.extend_from_slice(&bytes[..needed]);
        hash.update(&bytes[..count]);
        file.write_all(&bytes[..count])?;
    }
    Ok((file, hash, signature, size))
}

fn detect_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

// 流式移除 JSON 对 Base64 的斜杠转义，不复制大字符串。
#[path = "media_string.rs"]
mod string;
use string::JsonStringReader;
