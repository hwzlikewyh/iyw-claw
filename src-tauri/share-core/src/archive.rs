use crate::{invalid, safe_name, Result, MAX_TEXT_BYTES};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_ENTRIES: usize = 1000;
const MAX_EXPANDED_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_ENTRY_BYTES: u64 = 512 * 1024 * 1024;

pub(crate) struct ExtractionBudget {
    bytes: u64,
    entries: usize,
}

impl Default for ExtractionBudget {
    fn default() -> Self {
        Self {
            bytes: MAX_EXPANDED_BYTES,
            entries: MAX_ENTRIES,
        }
    }
}

pub(crate) struct Extracted {
    pub original_name: String,
    pub path: PathBuf,
}

pub(crate) fn extract(
    source: &Path,
    destination: &Path,
    budget: &mut ExtractionBudget,
) -> Result<Vec<Extracted>> {
    let mut archive = zip::ZipArchive::new(fs::File::open(source)?)?;
    if archive.len() > budget.entries {
        return Err(invalid("聊天归档内的文件超过 1000 个"));
    }
    budget.entries -= archive.len();
    let mut expanded = 0u64;
    let mut files = Vec::new();
    fs::create_dir_all(destination)?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        validate_entry(&entry)?;
        if entry.is_dir() {
            continue;
        }
        expanded = expanded.saturating_add(entry.size());
        if expanded > MAX_EXPANDED_BYTES || entry.size() > budget.bytes {
            return Err(invalid("分享批次解压后超过 1 GiB"));
        }
        budget.bytes -= entry.size();
        let basename = entry.name().rsplit('/').next().unwrap_or("file");
        // 不使用归档内部路径创建目录；编号隔离大小写、Unicode 与同名冲突。
        let path = destination.join(format!("{index:04}-{}", safe_name(basename)));
        let mut output = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)?;
        let copied = std::io::copy(&mut (&mut entry).take(MAX_ENTRY_BYTES + 1), &mut output)?;
        output.flush()?;
        output.sync_all()?;
        if copied > MAX_ENTRY_BYTES || copied != entry.size() {
            return Err(invalid("聊天归档内文件大小不匹配"));
        }
        files.push(Extracted {
            original_name: entry.name().to_owned(),
            path,
        });
    }
    if files.is_empty() {
        return Err(invalid("聊天归档中没有可读取的文件"));
    }
    Ok(files)
}

fn validate_entry(entry: &zip::read::ZipFile<'_>) -> Result<()> {
    let file_type = entry.unix_mode().unwrap_or(0) & 0o170000;
    if entry.enclosed_name().is_none()
        || entry.name().contains(['\\', ':'])
        || entry.name().chars().any(char::is_control)
        || entry.name().len() > 2048
        || !matches!(file_type, 0 | 0o040000 | 0o100000)
        || !matches!(
            entry.compression(),
            zip::CompressionMethod::Stored | zip::CompressionMethod::Deflated
        )
        || entry.size() > MAX_ENTRY_BYTES
        || (entry.is_dir() && entry.size() != 0)
    {
        return Err(invalid("聊天归档含不支持的路径、文件类型或压缩格式"));
    }
    Ok(())
}

pub(crate) fn read_text(path: &Path, name: &str) -> Result<Option<String>> {
    if !name.to_lowercase().ends_with(".txt") || fs::metadata(path)?.len() > MAX_TEXT_BYTES {
        return Ok(None);
    }
    Ok(String::from_utf8(fs::read(path)?)
        .ok()
        .map(|text| text.trim_start_matches('\u{feff}').replace("\r\n", "\n")))
}
