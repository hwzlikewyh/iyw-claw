//! Tiny blocking HTTP client used by `mark`/`unmark`/`marks`/`goto` to talk
//! to a running watch server. Built on `std::net::TcpStream` to avoid pulling
//! in a full async HTTP client dependency for what amounts to four short
//! JSON RPCs.

use handler_common::HandlerError;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Resolve which port to hit: explicit override, or the default watch port.
pub fn resolve_port(port: Option<u16>) -> u16 {
    port.unwrap_or(crate::watch::DEFAULT_PORT)
}

/// Default document id: the file stem, matching how `watch` derives ids.
pub fn default_id(file: &str) -> String {
    std::path::Path::new(file)
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "default".to_string())
}

/// Send a POST with a JSON body to `/<path>` on the watch server.
/// Returns the parsed JSON body of the response, with the standard
/// `{ "result": ..., "error": null }` envelope unwrapped to `result`.
pub fn post_json(
    port: u16,
    path: &str,
    body: &serde_json::Value,
) -> Result<serde_json::Value, HandlerError> {
    let body_str = serde_json::to_string(body).unwrap_or_else(|_| "{}".to_string());
    let request = format!(
        "POST {} HTTP/1.1\r\n\
         Host: 127.0.0.1\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n\
         {}",
        path,
        body_str.len(),
        body_str
    );
    let raw = send_raw(port, request.as_bytes())?;
    let parsed = parse_json_body(&raw)?;
    Ok(unwrap_envelope(parsed))
}

/// Send a GET to `/<path>` and return the parsed JSON body, envelope unwrapped.
pub fn get_json(port: u16, path: &str) -> Result<serde_json::Value, HandlerError> {
    let request = format!(
        "GET {} HTTP/1.1\r\n\
         Host: 127.0.0.1\r\n\
         Connection: close\r\n\
         \r\n",
        path
    );
    let raw = send_raw(port, request.as_bytes())?;
    let parsed = parse_json_body(&raw)?;
    Ok(unwrap_envelope(parsed))
}

/// If `value` is `{ "result": X, "error": null }`, return X. Otherwise pass
/// through. Surface errors that the server reported via the envelope.
fn unwrap_envelope(value: serde_json::Value) -> serde_json::Value {
    if let Some(obj) = value.as_object() {
        if let Some(err) = obj.get("error").and_then(|v| v.as_str()) {
            if !err.is_empty() {
                eprintln!("watch server reported error: {}", err);
            }
        }
        if let Some(result) = obj.get("result") {
            return result.clone();
        }
    }
    value
}

fn send_raw(port: u16, request: &[u8]) -> Result<Vec<u8>, HandlerError> {
    let addr = format!("127.0.0.1:{}", port);
    let socket_addr: std::net::SocketAddr = addr
        .parse()
        .map_err(|e| HandlerError::OperationFailed(format!("invalid address: {}", e)))?;
    let mut stream = TcpStream::connect_timeout(&socket_addr, CONNECT_TIMEOUT).map_err(|e| {
        HandlerError::OperationFailed(format!(
            "no watch server on port {}: {}. Start with `officecli watch <file> --port {}`.",
            port, e, port
        ))
    })?;
    stream.set_read_timeout(Some(READ_TIMEOUT)).ok();
    stream.set_write_timeout(Some(READ_TIMEOUT)).ok();
    stream
        .write_all(request)
        .map_err(|e| HandlerError::OperationFailed(format!("write: {}", e)))?;
    let mut buf = Vec::with_capacity(4096);
    stream
        .read_to_end(&mut buf)
        .map_err(|e| HandlerError::OperationFailed(format!("read: {}", e)))?;
    Ok(buf)
}

/// Split a raw HTTP response into status + body, and JSON-parse the body.
fn parse_json_body(raw: &[u8]) -> Result<serde_json::Value, HandlerError> {
    let text = String::from_utf8_lossy(raw);
    let body_start = text
        .find("\r\n\r\n")
        .map(|p| p + 4)
        .or_else(|| text.find("\n\n").map(|p| p + 2))
        .unwrap_or(text.len());
    let head = &text[..body_start.min(text.len())];
    let status_line = head.lines().next().unwrap_or("");
    if !status_line.contains(" 200") && !status_line.contains(" 204") {
        return Err(HandlerError::OperationFailed(format!(
            "watch server returned: {}",
            status_line
        )));
    }
    let body = &text[body_start.min(text.len())..];
    // Chunked-transfer-encoding responses wrap JSON in chunk-size lines.
    let body = if head
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        dechunk(body)
    } else {
        body.to_string()
    };
    serde_json::from_str::<serde_json::Value>(&body).map_err(|e| {
        HandlerError::OperationFailed(format!(
            "could not parse watch response: {} (body: {})",
            e, body
        ))
    })
}

fn dechunk(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s.trim_start_matches('\u{feff}');
    loop {
        let nl;
        let size_str = match rest.find("\r\n") {
            Some(p) => {
                nl = p;
                &rest[..p]
            }
            None => break,
        };
        let size = match usize::from_str_radix(size_str.trim(), 16) {
            Ok(n) => n,
            Err(_) => break,
        };
        if size == 0 {
            break;
        }
        rest = &rest[nl + 2..];
        if rest.len() < size {
            break;
        }
        out.push_str(&rest[..size]);
        rest = &rest[size + 2..];
    }
    out
}
