use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;

use super::{invalid, io_error};
use crate::app_error::AppCommandError;
use crate::logging::beijing;

const MAX_SOURCE_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_RECORD_BYTES: u64 = 4 * 1024 * 1024;
const PREPARATION_TIMEOUT: Duration = Duration::from_secs(120);
const LOG_PREFIXES: [&str; 2] = ["iyw-claw", "iyw-claw-server"];

pub(super) struct LogSource {
    pub name: String,
    pub path: PathBuf,
    pub size_bytes: u64,
}

#[derive(Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LogCounts {
    pub records: u64,
    pub skipped: u64,
}

#[derive(Deserialize)]
struct RecordTime<'a> {
    #[serde(borrow)]
    timestamp: Option<&'a str>,
    timestamp_ms: Option<i64>,
}

pub(super) fn parse_date(value: &str) -> Result<NaiveDate, AppCommandError> {
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .ok()
        .filter(|date| date.to_string() == value && *date <= beijing::now().date_naive());
    date.ok_or_else(|| invalid("Select a valid date no later than today in Beijing", "date"))
}

pub(super) fn sources(date: NaiveDate) -> Result<Vec<LogSource>, AppCommandError> {
    let directory = match fs::canonicalize(crate::paths::iyw_claw_logs_root()) {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(io_error("Log directory is unavailable", error)),
    };
    let mut files = Vec::new();
    for day in date.pred_opt().into_iter().chain(std::iter::once(date)) {
        for prefix in LOG_PREFIXES {
            let name = format!("{prefix}.{day}.log");
            if let Some(file) = inspect(directory.join(&name), name)? {
                files.push(file);
            }
        }
    }
    if files
        .iter()
        .fold(0_u64, |total, file| total.saturating_add(file.size_bytes))
        > MAX_SOURCE_BYTES
    {
        return Err(invalid("Selected source logs exceed 1 GiB", "logsTooLarge"));
    }
    Ok(files)
}

fn inspect(path: PathBuf, name: String) -> Result<Option<LogSource>, AppCommandError> {
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_error("Failed to inspect a log file", error)),
    };
    if !metadata.file_type().is_file() {
        return Err(invalid("Log source must be a regular file", "readFailed"));
    }
    Ok(Some(LogSource {
        name,
        path,
        size_bytes: metadata.len(),
    }))
}

fn open_source(source: &LogSource) -> Result<BufReader<std::io::Take<File>>, AppCommandError> {
    let file =
        File::open(&source.path).map_err(|error| io_error("Failed to open log file", error))?;
    let actual_path =
        fs::canonicalize(&source.path).map_err(|error| io_error("Log file moved", error))?;
    if actual_path != source.path
        || !file
            .metadata()
            .map_err(|e| io_error("Failed to inspect log", e))?
            .is_file()
    {
        return Err(invalid(
            "Log source changed during report preparation",
            "readFailed",
        ));
    }
    Ok(BufReader::new(file.take(source.size_bytes)))
}

pub(super) fn copy_day(
    source: &LogSource,
    writer: &mut impl Write,
    selection: (NaiveDate, Instant),
) -> Result<LogCounts, AppCommandError> {
    let mut reader = open_source(source)?;
    let mut counts = LogCounts::default();
    let mut line = Vec::new();
    loop {
        if selection.1.elapsed() > PREPARATION_TIMEOUT {
            return Err(invalid(
                "Log preparation exceeded its time limit",
                "preparationTimeout",
            ));
        }
        line.clear();
        let read = reader
            .by_ref()
            .take(MAX_RECORD_BYTES + 1)
            .read_until(b'\n', &mut line)
            .map_err(|error| io_error("Failed to read log file", error))?;
        if read == 0 {
            break;
        }
        if read as u64 > MAX_RECORD_BYTES {
            return Err(invalid("A log record exceeds 4 MiB", "logsTooLarge"));
        }
        match record_date(&line) {
            Some(date) if date == selection.0 => {
                write_record(writer, &line)?;
                counts.records += 1;
            }
            None => counts.skipped += 1,
            _ => {}
        }
    }
    if reader.get_ref().limit() != 0 {
        return Err(invalid(
            "Log file was truncated during report preparation",
            "readFailed",
        ));
    }
    Ok(counts)
}

fn write_record(writer: &mut impl Write, line: &[u8]) -> Result<(), AppCommandError> {
    writer
        .write_all(line)
        .map_err(super::archive::write_error)?;
    if !line.ends_with(b"\n") {
        writer
            .write_all(b"\n")
            .map_err(super::archive::write_error)?;
    }
    Ok(())
}

fn record_date(line: &[u8]) -> Option<NaiveDate> {
    let record: RecordTime<'_> = serde_json::from_slice(line).ok()?;
    let timestamp = record
        .timestamp
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc))
        .or_else(|| {
            record
                .timestamp_ms
                .and_then(DateTime::<Utc>::from_timestamp_millis)
        })?;
    Some(timestamp.with_timezone(&beijing::offset()).date_naive())
}
