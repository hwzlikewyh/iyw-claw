use crate::{checked_file, invalid, safe_name, Result, ShareBatch, ShareItem, ShareSummary};
use crate::{MAX_BATCH_BYTES, MAX_FILES, MAX_FILE_BYTES};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub struct Inbox {
    pub root: PathBuf,
}

impl Inbox {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn begin(&self) -> Result<BatchWriter> {
        let id = Uuid::new_v4().to_string();
        let directory = self.root.join("Staging").join(&id);
        fs::create_dir_all(directory.join("files"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
        }
        Ok(BatchWriter {
            root: self.root.clone(),
            directory,
            batch: ShareBatch {
                schema_version: 1,
                id,
                created_at: chrono::Utc::now(),
                items: vec![],
            },
            committed: false,
        })
    }

    pub fn list(&self) -> Result<Vec<ShareSummary>> {
        let mut batches = Vec::new();
        for folder in ["Ready"] {
            let directory = self.root.join(folder);
            if !directory.exists() {
                continue;
            }
            for entry in fs::read_dir(directory)? {
                let entry = entry?;
                let id = entry.file_name().to_string_lossy().into_owned();
                if entry.file_type()?.is_dir() && Uuid::parse_str(&id).is_ok() {
                    let (batch, error) = match self.read(&id) {
                        Ok(batch) => (batch, None),
                        Err(error) => (
                            ShareBatch {
                                schema_version: 1,
                                id,
                                created_at: entry.metadata()?.modified()?.into(),
                                items: vec![],
                            },
                            Some(error.to_string()),
                        ),
                    };
                    let imported = entry.path().join("imported.json").is_file();
                    batches.push(ShareSummary {
                        batch,
                        imported,
                        error,
                    });
                }
            }
        }
        batches.sort_by(|a, b| {
            a.imported
                .cmp(&b.imported)
                .then_with(|| b.batch.created_at.cmp(&a.batch.created_at))
        });
        Ok(batches)
    }

    pub fn directory(&self, id: &str) -> Result<PathBuf> {
        if Uuid::parse_str(id).is_err() {
            return Err(invalid("无效的分享批次编号"));
        }
        for folder in ["Ready"] {
            let path = self.root.join(folder).join(id);
            if path.exists() {
                let canonical = path.canonicalize()?;
                if fs::symlink_metadata(&path)?.file_type().is_symlink()
                    || canonical.parent() != Some(self.root.join(folder).canonicalize()?.as_path())
                {
                    return Err(invalid("分享目录超出受管范围"));
                }
                return Ok(canonical);
            }
        }
        Err(invalid("分享记录不存在"))
    }

    pub fn read(&self, id: &str) -> Result<ShareBatch> {
        let directory = self.directory(id)?;
        let file = checked_file(&directory, "manifest.json")?;
        if fs::metadata(&file)?.len() > 64 * 1024 {
            return Err(invalid("分享清单过大"));
        }
        let batch: ShareBatch = serde_json::from_slice(&fs::read(file)?)?;
        if batch.id != id
            || batch.schema_version != 1
            || batch.items.is_empty()
            || batch.items.len() > MAX_FILES
        {
            return Err(invalid("分享清单版本或文件数无效"));
        }
        let mut total = 0u64;
        for item in &batch.items {
            let path = checked_file(&directory, &item.relative_path)?;
            let size = fs::metadata(path)?.len();
            total = total.saturating_add(size);
            if size != item.size || size == 0 || size > MAX_FILE_BYTES || total > MAX_BATCH_BYTES {
                return Err(invalid("分享文件不完整或超出大小限制"));
            }
        }
        Ok(batch)
    }

    pub fn complete(&self, id: &str) -> Result<()> {
        self.read(id)?;
        // 草稿引用的绝对路径必须稳定，消费后只写标记，不移动文件。
        let path = self.directory(id)?.join("imported.json");
        if path.exists() {
            return Ok(());
        }
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)?;
        file.write_all(b"{\"imported\":true}")?;
        file.sync_all()?;
        Ok(())
    }
}

pub struct BatchWriter {
    root: PathBuf,
    directory: PathBuf,
    batch: ShareBatch,
    committed: bool,
}

impl BatchWriter {
    pub fn destination(&self, name: &str) -> Result<PathBuf> {
        if self.batch.items.len() >= MAX_FILES {
            return Err(invalid("一次最多接收 32 个分享文件"));
        }
        Ok(self.directory.join("files").join(format!(
            "{:04}-{}",
            self.batch.items.len(),
            safe_name(name)
        )))
    }

    pub fn record_file(&mut self, path: &Path, name: &str) -> Result<()> {
        if self.destination(name)? != path {
            return Err(invalid("接收文件路径不匹配"));
        }
        let metadata = fs::symlink_metadata(path)?;
        let total = self
            .batch
            .items
            .iter()
            .map(|item| item.size)
            .sum::<u64>()
            .saturating_add(metadata.len());
        if !metadata.is_file()
            || metadata.len() == 0
            || metadata.len() > MAX_FILE_BYTES
            || total > MAX_BATCH_BYTES
        {
            return Err(invalid("文件为空或分享批次超过 512 MiB"));
        }
        sync_received_file(path)?;
        self.batch.items.push(ShareItem {
            name: safe_name(name),
            relative_path: path
                .strip_prefix(&self.directory)
                .map_err(|_| invalid("无效的接收目录"))?
                .to_string_lossy()
                .replace('\\', "/"),
            size: metadata.len(),
        });
        Ok(())
    }

    pub fn copy_file(&mut self, source: &Path, name: &str) -> Result<()> {
        let metadata = fs::symlink_metadata(source)?;
        if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
            return Err(invalid("请选择不超过 512 MiB 的普通文件"));
        }
        let destination = self.destination(name)?;
        let mut output = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&destination)?;
        let copied = std::io::copy(
            &mut fs::File::open(source)?.take(MAX_FILE_BYTES + 1),
            &mut output,
        )?;
        output.flush()?;
        if copied > MAX_FILE_BYTES {
            return Err(invalid("分享文件超过 512 MiB"));
        }
        self.record_file(&destination, name)
    }

    pub fn commit(mut self) -> Result<ShareBatch> {
        if self.batch.items.is_empty() {
            return Err(invalid("分享中没有文件"));
        }
        let mut manifest = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(self.directory.join("manifest.json"))?;
        manifest.write_all(&serde_json::to_vec_pretty(&self.batch)?)?;
        manifest.sync_all()?;
        fs::create_dir_all(self.root.join("Ready"))?;
        fs::rename(
            &self.directory,
            self.root.join("Ready").join(&self.batch.id),
        )?;
        self.committed = true;
        Ok(self.batch.clone())
    }
}

impl Drop for BatchWriter {
    fn drop(&mut self) {
        // 只清理本次实例创建、尚未发布的暂存目录。
        if !self.committed {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }
}

fn sync_received_file(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(windows)]
    {
        let mut permissions = fs::metadata(path)?.permissions();
        if permissions.readonly() {
            permissions.set_readonly(false);
            fs::set_permissions(path, permissions)?;
        }
    }
    // Windows 的 FlushFileBuffers 要求写权限，不能通过只读句柄同步。
    fs::OpenOptions::new().write(true).open(path)?.sync_all()?;
    Ok(())
}
