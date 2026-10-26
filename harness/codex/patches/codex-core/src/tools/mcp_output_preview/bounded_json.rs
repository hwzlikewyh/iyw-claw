use serde::Serialize;
use std::io::{self, Write};

const INITIAL_CAPTURE_CAPACITY: usize = 16 * 1024;

pub(super) fn encode<T: Serialize>(value: &T, limit: usize) -> Option<Vec<u8>> {
    let mut output = BoundedOutput {
        bytes: Vec::with_capacity(limit.min(INITIAL_CAPTURE_CAPACITY)),
        limit,
    };
    serde_json::to_writer(&mut output, value).ok()?;
    Some(output.bytes)
}

pub(super) fn fits<T: Serialize>(value: &T, limit: usize) -> bool {
    serde_json::to_writer(&mut Counter { written: 0, limit }, value).is_ok()
}

struct BoundedOutput {
    bytes: Vec<u8>,
    limit: usize,
}

impl Write for BoundedOutput {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        let size = self
            .bytes
            .len()
            .checked_add(input.len())
            .filter(|size| *size <= self.limit)
            .ok_or_else(|| io::Error::other("MCP result exceeds capture budget"))?;
        if size > self.bytes.capacity() {
            let capacity = size
                .max(self.bytes.capacity().saturating_mul(2))
                .min(self.limit);
            self.bytes
                .try_reserve_exact(capacity - self.bytes.len())
                .map_err(|_| io::Error::other("MCP result capture allocation failed"))?;
        }
        self.bytes.extend_from_slice(input);
        Ok(input.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct Counter {
    written: usize,
    limit: usize,
}

impl Write for Counter {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        self.written = self
            .written
            .checked_add(input.len())
            .filter(|written| *written <= self.limit)
            .ok_or_else(|| io::Error::other("JSON exceeds preview budget"))?;
        Ok(input.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
