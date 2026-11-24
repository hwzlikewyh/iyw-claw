use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

use super::super::WatchError;

const MAX_SOURCE_BYTES: u64 = 256 * 1024 * 1024;
const SOURCE_RETRY_DELAY: Duration = Duration::from_millis(250);
const SOURCE_ATTEMPTS: usize = 4;

pub(super) async fn convert(
    file: &Path,
    root: Arc<TempDir>,
    cancel: CancellationToken,
) -> Result<String, WatchError> {
    for attempt in 1..=SOURCE_ATTEMPTS {
        let result = convert_once(file, root.clone(), cancel.clone()).await;
        if !matches!(&result, Err(WatchError::Io(_) | WatchError::FileNotReady))
            || attempt == SOURCE_ATTEMPTS
        {
            return result;
        }
        tokio::select! {
            _ = cancel.cancelled() => return Err(WatchError::StartFailed("Preview closed".into())),
            _ = tokio::time::sleep(SOURCE_RETRY_DELAY) => {}
        }
    }
    unreachable!("source retry count is nonzero")
}

async fn convert_once(
    file: &Path,
    root: Arc<TempDir>,
    cancel: CancellationToken,
) -> Result<String, WatchError> {
    let permit = tokio::select! {
        _ = cancel.cancelled() => return Err(WatchError::StartFailed("Preview closed".into())),
        permit = super::IMPORT_SLOTS.acquire() => permit.map_err(|error| WatchError::Io(error.to_string()))?,
    };
    let file = file.to_owned();
    let generation = uuid::Uuid::new_v4().simple().to_string();
    let next = generation.clone();
    let started = Instant::now();
    let task = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let result = convert_sync(&file, &root.path().join(&next), &cancel);
        if result.is_err() {
            let _ = std::fs::remove_dir_all(root.path().join(&next));
        }
        result
    });
    let result = task
        .await
        .map_err(|error| WatchError::Io(error.to_string()))?;
    result?;
    tracing::info!(
        duration_ms = started.elapsed().as_millis() as u64,
        "[office-preview] embedded document imported"
    );
    Ok(generation)
}

fn convert_sync(file: &Path, output: &Path, cancel: &CancellationToken) -> Result<(), WatchError> {
    if cancel.is_cancelled() {
        return Err(WatchError::StartFailed("Preview closed".into()));
    }
    // 每次更新重新约束真实路径，防止生成期间被换成指向工作区外的符号链接。
    if std::fs::canonicalize(file)? != file {
        return Err(WatchError::OutsideRoot);
    }
    let before = super::super::file_state(file).ok_or(WatchError::FileNotReady)?;
    if before.0 > MAX_SOURCE_BYTES {
        return Err(WatchError::StartFailed(
            "Document exceeds the 256 MiB preview limit".into(),
        ));
    }
    let root = output.parent().ok_or(WatchError::FileNotReady)?;
    let source = copy_source(file, root)?;
    let mut options = hcd_formats::ImportOptions::new("preview");
    options.storage_codec = hcd_core::StorageCodec::None;
    let result = hcd_formats::import_document(&source, output, &options, |_| {
        if cancel.is_cancelled() {
            return Err(hcd_core::HcdError::Unsupported("Preview closed".into()));
        }
        Ok(())
    })
    .and_then(|manifest| {
        iyw_office_engine::preview_styles::write(&source, output)
            .map_err(|error| hcd_core::HcdError::Unsupported(format!("Preview layout: {error}")))?;
        Ok(manifest)
    });
    let _ = std::fs::remove_file(source);
    if super::super::file_state(file) != Some(before) {
        return Err(WatchError::FileNotReady);
    }
    result.map_err(|error| WatchError::StartFailed(error.to_string()))?;
    Ok(())
}

fn copy_source(file: &Path, root: &Path) -> Result<PathBuf, WatchError> {
    let extension = file
        .extension()
        .and_then(|value| value.to_str())
        .ok_or(WatchError::NotOffice)?;
    let copy = root.join(format!(
        "source-{}.{}",
        uuid::Uuid::new_v4().simple(),
        extension
    ));
    if let Err(error) = std::fs::copy(file, &copy) {
        let _ = std::fs::remove_file(&copy);
        return Err(error.into());
    }
    Ok(copy)
}
