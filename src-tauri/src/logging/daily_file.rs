use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use chrono::NaiveDate;

use super::beijing;

pub(super) struct DailyFile {
    directory: PathBuf,
    prefix: String,
    retained_files: usize,
    date: NaiveDate,
    file: File,
}

impl DailyFile {
    pub(super) fn open(directory: &Path, prefix: &str, retained_files: usize) -> io::Result<Self> {
        let date = beijing::now().date_naive();
        let file = open_file(directory, prefix, date)?;
        let writer = Self {
            directory: directory.to_path_buf(),
            prefix: prefix.to_string(),
            retained_files,
            date,
            file,
        };
        writer.prune();
        Ok(writer)
    }

    fn rotate(&mut self) -> io::Result<()> {
        let date = beijing::now().date_naive();
        if date == self.date {
            return Ok(());
        }
        let file = open_file(&self.directory, &self.prefix, date)?;
        self.file.flush()?;
        self.file = file;
        self.date = date;
        self.prune();
        Ok(())
    }

    fn prune(&self) {
        let result = prune_files(
            &self.directory,
            &self.prefix,
            (self.date, self.retained_files),
        );
        if let Err(error) = result {
            eprintln!("[logging] failed to enforce daily file retention: {error}");
        }
    }
}

impl Write for DailyFile {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.rotate()?;
        self.file.write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

fn open_file(directory: &Path, prefix: &str, date: NaiveDate) -> io::Result<File> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(directory.join(format!("{prefix}.{date}.log")))
}

fn file_date(name: &str, prefix: &str) -> Option<NaiveDate> {
    let value = name
        .strip_prefix(prefix)?
        .strip_prefix('.')?
        .strip_suffix(".log")?;
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()?;
    (date.to_string() == value).then_some(date)
}

fn prune_files(directory: &Path, prefix: &str, retention: (NaiveDate, usize)) -> io::Result<()> {
    let mut files = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        if let Some(date) = file_date(&entry.file_name().to_string_lossy(), prefix) {
            files.push((date, entry.path()));
        }
    }
    files.sort_by_key(|(date, _)| *date);
    let remove_count = files.len().saturating_sub(retention.1);
    // 保留当前正在写入的文件，避免系统时钟回拨时删除活动日志。
    let stale = files.into_iter().filter(|(date, _)| *date != retention.0);
    for (_, path) in stale.take(remove_count) {
        fs::remove_file(path)?;
    }
    Ok(())
}
