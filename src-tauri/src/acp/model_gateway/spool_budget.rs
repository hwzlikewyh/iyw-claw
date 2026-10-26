use std::io::{self, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use super::{error::GatewayError, limits::MAX_BODY_BYTES};

const TOTAL_SPOOL_BYTES: u64 = 1024 * 1024 * 1024;
static RESERVED_BYTES: AtomicU64 = AtomicU64::new(0);

#[derive(Default)]
pub(super) struct SpoolBudget(AtomicU64);

impl SpoolBudget {
    pub fn reserve(&self, amount: u64) -> Result<(), GatewayError> {
        RESERVED_BYTES
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(amount)
                    .filter(|next| *next <= TOTAL_SPOOL_BYTES)
            })
            .map_err(|_| GatewayError::unavailable("Local request storage budget exhausted"))?;
        self.0.fetch_add(amount, Ordering::Relaxed);
        Ok(())
    }

    fn release(&self, amount: u64) {
        self.0.fetch_sub(amount, Ordering::Relaxed);
        RESERVED_BYTES.fetch_sub(amount, Ordering::AcqRel);
    }
}

impl Drop for SpoolBudget {
    fn drop(&mut self) {
        RESERVED_BYTES.fetch_sub(self.0.load(Ordering::Relaxed), Ordering::AcqRel);
    }
}

pub(super) struct SpoolWriter {
    pub file: std::fs::File,
    pub budget: Arc<SpoolBudget>,
    pub size: u64,
}

impl SpoolWriter {
    pub fn new() -> Result<Self, GatewayError> {
        Ok(Self {
            file: tempfile::tempfile()?,
            budget: Arc::default(),
            size: 0,
        })
    }
}

impl Write for SpoolWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() as u64 > MAX_BODY_BYTES.saturating_sub(self.size) {
            return Err(io::Error::other(
                "Prepared request exceeds the storage limit",
            ));
        }
        self.budget
            .reserve(bytes.len() as u64)
            .map_err(|_| io::Error::other("Local request storage budget exhausted"))?;
        let written = match self.file.write(bytes) {
            Ok(written) => written,
            Err(error) => {
                self.budget.release(bytes.len() as u64);
                return Err(error);
            }
        };
        self.budget.release((bytes.len() - written) as u64);
        self.size += written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

pub(super) struct BudgetedReader<R> {
    pub inner: R,
    pub _budget: Arc<SpoolBudget>,
}

impl<R: io::Read> io::Read for BudgetedReader<R> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.inner.read(bytes)
    }
}
