use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::time::Instant;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

use super::{files, invalid, io_error, screenshots, ReportRequest};
use crate::app_error::AppCommandError;
use crate::logging::beijing;

pub(super) const MAX_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;
const HASH_BUFFER_BYTES: usize = 64 * 1024;

pub(super) struct PreparedReport {
    pub file: File,
    pub date: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub log_records: u64,
    pub skipped_records: u64,
}

struct BoundedFile {
    file: File,
    position: u64,
}

impl Write for BoundedFile {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.position.saturating_add(bytes.len() as u64) > MAX_ARCHIVE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                "Report archive exceeds 256 MiB",
            ));
        }
        let written = self.file.write(bytes)?;
        self.position += written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

impl Seek for BoundedFile {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.position = self.file.seek(position)?;
        Ok(self.position)
    }
}

pub(super) fn prepare(request: ReportRequest) -> Result<PreparedReport, AppCommandError> {
    let date = files::parse_date(&request.date)?;
    let screenshots = screenshots::decode(request.screenshots)?;
    let file = tempfile::tempfile().map_err(write_error)?;
    let mut archive = ZipWriter::new(BoundedFile { file, position: 0 });
    let (log_files, counts) = append_logs(&mut archive, date)?;
    let screenshot_files = append_screenshots(&mut archive, &screenshots)?;
    let manifest = json!({
        "schemaVersion": 1, "createdAt": beijing::now().to_rfc3339(),
        "date": request.date, "timezone": "+08:00", "description": request.description.trim(),
        "appVersion": env!("CARGO_PKG_VERSION"), "os": std::env::consts::OS, "arch": std::env::consts::ARCH,
        "logs": log_files, "logRecords": counts.records, "skippedRecords": counts.skipped,
        "screenshots": screenshot_files,
    });
    start_entry(&mut archive, "report.json")?;
    serde_json::to_writer_pretty(&mut archive, &manifest)
        .map_err(|error| io_error("Failed to write report manifest", error))?;
    finish(archive, request.date, counts)
}

fn append_logs(
    archive: &mut ZipWriter<BoundedFile>,
    date: chrono::NaiveDate,
) -> Result<(Vec<Value>, files::LogCounts), AppCommandError> {
    let started = Instant::now();
    let mut counts = files::LogCounts::default();
    let mut log_files = Vec::new();
    for source in files::sources(date)? {
        start_entry(archive, &format!("logs/{}", source.name))?;
        let copied = files::copy_day(&source, archive, (date, started))?;
        counts.records += copied.records;
        counts.skipped += copied.skipped;
        log_files.push(
            json!({"name": source.name, "sourceBytes": source.size_bytes, "included": copied}),
        );
    }
    Ok((log_files, counts))
}

fn append_screenshots(
    archive: &mut ZipWriter<BoundedFile>,
    screenshots: &[screenshots::Screenshot],
) -> Result<Vec<Value>, AppCommandError> {
    let mut files = Vec::new();
    for screenshot in screenshots {
        start_entry(archive, &screenshot.path)?;
        archive.write_all(&screenshot.bytes).map_err(write_error)?;
        files.push(json!({"path": screenshot.path, "sizeBytes": screenshot.bytes.len()}));
    }
    Ok(files)
}

fn finish(
    archive: ZipWriter<BoundedFile>,
    date: String,
    counts: files::LogCounts,
) -> Result<PreparedReport, AppCommandError> {
    let mut file = archive.finish().map_err(zip_error)?.file;
    let size_bytes = file.metadata().map_err(write_error)?.len();
    file.rewind().map_err(write_error)?;
    let sha256 = digest(&mut file)?;
    file.rewind().map_err(write_error)?;
    tracing::info!(
        size_bytes,
        records = counts.records,
        skipped_records = counts.skipped,
        "[log-report] archive ready"
    );
    Ok(PreparedReport {
        file,
        date,
        size_bytes,
        sha256,
        log_records: counts.records,
        skipped_records: counts.skipped,
    })
}

fn start_entry(archive: &mut ZipWriter<BoundedFile>, name: &str) -> Result<(), AppCommandError> {
    archive
        .start_file(
            name,
            SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
        )
        .map_err(zip_error)
}

fn digest(file: &mut File) -> Result<String, AppCommandError> {
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; HASH_BUFFER_BYTES];
    loop {
        let count = file.read(&mut buffer).map_err(write_error)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

pub(super) fn write_error(error: io::Error) -> AppCommandError {
    if error.kind() == io::ErrorKind::FileTooLarge {
        return invalid("Report archive exceeds 256 MiB", "archiveTooLarge");
    }
    io_error("Failed to prepare report archive", error)
}

fn zip_error(error: zip::result::ZipError) -> AppCommandError {
    match error {
        zip::result::ZipError::Io(error) => write_error(error),
        error => io_error("Failed to prepare report archive", error),
    }
}
