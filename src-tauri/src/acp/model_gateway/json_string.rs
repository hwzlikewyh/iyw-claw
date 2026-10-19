use std::io::BufRead;

use super::{
    error::GatewayError,
    json_scan::{Parser, Value},
};

const PREFIX_BYTES: usize = 1024;

impl Parser {
    pub(super) fn string(&mut self) -> Result<Value, GatewayError> {
        let start = self.offset;
        self.byte()?;
        let mut small = Vec::with_capacity(64);
        loop {
            let bytes = self.input.fill_buf()?;
            let count = bytes
                .iter()
                .position(|byte| matches!(byte, b'"' | b'\\') || *byte < b' ')
                .unwrap_or(bytes.len());
            if count == 0 {
                if self.string_boundary(&mut small)? {
                    break;
                }
            } else {
                let keep = count.min(PREFIX_BYTES.saturating_sub(small.len()));
                small.extend_from_slice(&bytes[..keep]);
                self.input.consume(count);
                self.offset += count as u64;
            }
        }
        let inline = small.starts_with(b"data:")
            || decoded_prefix(&small).is_some_and(|prefix| prefix.starts_with("data:"));
        let text = if self.offset - start <= PREFIX_BYTES as u64 {
            Some(decode(&small)?)
        } else {
            None
        };
        Ok(Value {
            span: start..self.offset,
            text,
            inline,
            fields: Vec::new(),
            source_extra: Vec::new(),
        })
    }

    fn string_boundary(&mut self, prefix: &mut Vec<u8>) -> Result<bool, GatewayError> {
        match self.byte()? {
            b'"' => Ok(true),
            b'\\' => {
                let escape = self.byte()?;
                if !matches!(
                    escape,
                    b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' | b'u'
                ) {
                    return Err(GatewayError::input("Invalid JSON escape"));
                }
                if prefix.len() + 2 <= PREFIX_BYTES {
                    prefix.extend_from_slice(&[b'\\', escape]);
                }
                if escape == b'u' {
                    for _ in 0..4 {
                        let byte = self.byte()?;
                        if !byte.is_ascii_hexdigit() {
                            return Err(GatewayError::input("Invalid Unicode escape"));
                        }
                        if prefix.len() < PREFIX_BYTES {
                            prefix.push(byte);
                        }
                    }
                }
                Ok(false)
            }
            _ => Err(GatewayError::input("Invalid JSON string")),
        }
    }
}

fn decode(bytes: &[u8]) -> Result<String, GatewayError> {
    let mut encoded = Vec::with_capacity(bytes.len() + 2);
    encoded.push(b'"');
    encoded.extend_from_slice(bytes);
    encoded.push(b'"');
    serde_json::from_slice(&encoded)
        .map_err(|_| GatewayError::input("Invalid JSON string encoding"))
}

fn decoded_prefix(bytes: &[u8]) -> Option<String> {
    if !bytes.contains(&b'\\') {
        return None;
    }
    let mut end = bytes.len().min(64);
    while end > 0 {
        if let Ok(prefix) = decode(&bytes[..end]) {
            return Some(prefix);
        }
        end -= 1;
    }
    None
}
