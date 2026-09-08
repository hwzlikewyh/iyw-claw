//! 工具迁移的可恢复链接切换；事务记录始终先于来源目录改名。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const RECEIPT: &str = ".iyw-claw-runtime-migration.json";

#[derive(Serialize, Deserialize)]
struct Transaction {
    source: PathBuf,
    id: String,
}

pub(super) fn recover(source: &Path, target: &Path) -> io::Result<bool> {
    if !target.join(RECEIPT).is_file() {
        return Ok(false);
    }
    publish(source, target)?;
    Ok(true)
}

pub(super) fn prepare(source: &Path, staging: &Path) -> io::Result<()> {
    load_or_create(source, staging).map(|_| ())
}

pub(super) fn publish(source: &Path, target: &Path) -> io::Result<()> {
    let transaction = load_or_create(source, target)?;
    let parent = source
        .parent()
        .ok_or_else(|| io::Error::other("Runtime source has no parent"))?;
    let backup = parent.join(format!(".runtime-backup-{}", transaction.id));
    let link = parent.join(format!(".runtime-link-{}", transaction.id));
    if !same_directory(source, target) {
        if !source.exists() && backup.is_dir() {
            fs::rename(&backup, source)?;
        }
        if !same_directory(&link, target) {
            create_link(target, &link)?;
        }
        if let Err(error) = fs::rename(source, &backup) {
            return Err(io::Error::other(format!(
                "Runtime source backup failed: {error}"
            )));
        }
        if let Err(error) = fs::rename(&link, source) {
            let restore = fs::rename(&backup, source);
            return Err(io::Error::other(format!(
                "Runtime link activation failed: {error}; restore: {restore:?}"
            )));
        }
    }
    if backup.exists() {
        fs::remove_dir_all(&backup)?;
    }
    fs::remove_file(target.join(RECEIPT))?;
    tracing::info!(source = %source.display(), target = %target.display(),
        "[shared-runtime] committed recoverable compatibility link");
    Ok(())
}

fn load_or_create(source: &Path, target: &Path) -> io::Result<Transaction> {
    let receipt = target.join(RECEIPT);
    match fs::read(&receipt) {
        Ok(bytes) => {
            let transaction: Transaction =
                serde_json::from_slice(&bytes).map_err(io::Error::other)?;
            if transaction.source != source || uuid::Uuid::parse_str(&transaction.id).is_err() {
                return Err(io::Error::other(
                    "Shared runtime migration belongs to another source",
                ));
            }
            Ok(transaction)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let transaction = Transaction {
                source: source.to_path_buf(),
                id: uuid::Uuid::new_v4().to_string(),
            };
            let next = receipt.with_extension("next");
            fs::write(
                &next,
                serde_json::to_vec(&transaction).map_err(io::Error::other)?,
            )?;
            fs::OpenOptions::new().write(true).open(&next)?.sync_all()?;
            fs::rename(next, receipt)?;
            Ok(transaction)
        }
        Err(error) => Err(error),
    }
}

fn same_directory(first: &Path, second: &Path) -> bool {
    first
        .canonicalize()
        .ok()
        .zip(second.canonicalize().ok())
        .is_some_and(|(first, second)| first == second)
}

fn create_link(source: &Path, target: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        junction::create(source, target)
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(source, target)
    }
}
