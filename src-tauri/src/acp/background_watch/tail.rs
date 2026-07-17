use std::path::Path;

use std::io::{Read, Seek, SeekFrom};

pub(super) struct TranscriptTail {
    committed: u64,
    carry: Vec<u8>,
}

impl TranscriptTail {
    pub(super) fn from_offset(committed: u64) -> Self {
        Self {
            committed,
            carry: Vec::new(),
        }
    }

    pub(super) fn committed(&self) -> u64 {
        self.committed
    }

    pub(super) fn reset(&mut self, committed: u64) {
        self.committed = committed;
        self.carry.clear();
    }

    pub(super) fn read_new_lines(&mut self, path: &Path) -> std::io::Result<Vec<String>> {
        let mut file = std::fs::File::open(path)?;
        file.seek(SeekFrom::Start(self.committed + self.carry.len() as u64))?;
        let mut fresh = Vec::new();
        file.read_to_end(&mut fresh)?;
        self.carry.extend_from_slice(&fresh);

        let mut lines = Vec::new();
        while let Some(newline) = self.carry.iter().position(|byte| *byte == b'\n') {
            let rest = self.carry.split_off(newline + 1);
            let mut bytes = std::mem::replace(&mut self.carry, rest);
            bytes.pop();
            self.committed += newline as u64 + 1;
            if let Ok(line) = String::from_utf8(bytes) {
                lines.push(line);
            }
        }
        Ok(lines)
    }
}

pub(super) fn baseline_offset_since(path: &Path, epoch: std::time::SystemTime) -> Option<u64> {
    let bytes = std::fs::read(path).ok()?;
    let epoch: chrono::DateTime<chrono::Utc> = epoch.into();
    let mut offset = 0u64;
    for line in bytes.split_inclusive(|byte| *byte == b'\n') {
        if line.last() != Some(&b'\n') {
            break;
        }
        let start = offset;
        offset += line.len() as u64;
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(line) else {
            continue;
        };
        let record_type = value
            .get("type")
            .and_then(|kind| kind.as_str())
            .unwrap_or("");
        if record_type != "user" && record_type != "assistant" {
            continue;
        }
        let Some(timestamp) = value.get("timestamp").and_then(|value| value.as_str()) else {
            continue;
        };
        let Ok(timestamp) = chrono::DateTime::parse_from_rfc3339(timestamp) else {
            continue;
        };
        if timestamp.with_timezone(&chrono::Utc) >= epoch {
            return Some(start);
        }
    }
    Some(offset)
}
