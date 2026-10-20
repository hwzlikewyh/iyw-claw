use std::path::PathBuf;
use std::sync::Arc;

use tokio::io::AsyncWriteExt;

use super::{
    payload::{self, Envelope, Payload},
    Runtime,
};
use crate::chat_channel::{
    attachments::{ChannelAttachment, MAX_INBOUND_BYTES, MIB},
    error::ChatChannelError,
};

const MAX_SPOOL_BYTES: u64 = 512 * MIB;

pub(super) async fn stage(
    runtime: &Runtime,
    file: ChannelAttachment,
) -> Result<Payload, ChatChannelError> {
    file.validate(MAX_INBOUND_BYTES as u64)?;
    tokio::fs::create_dir_all(&runtime.spool)
        .await
        .map_err(|_| payload::failure("SPOOL_UNAVAILABLE"))?;
    check_capacity(runtime, file.byte_len()).await?;
    let key = uuid::Uuid::new_v4().to_string();
    let staged = tempfile::NamedTempFile::new_in(&runtime.spool)
        .map_err(|_| payload::failure("SPOOL_WRITE_FAILED"))?;
    let handle = staged
        .reopen()
        .map_err(|_| payload::failure("SPOOL_WRITE_FAILED"))?;
    let mut output = tokio::fs::File::from_std(handle);
    output
        .write_all(&file.bytes)
        .await
        .map_err(|_| payload::failure("SPOOL_WRITE_FAILED"))?;
    output
        .sync_all()
        .await
        .map_err(|_| payload::failure("SPOOL_WRITE_FAILED"))?;
    drop(output);
    staged
        .persist_noclobber(path(runtime, &key)?)
        .map_err(|_| payload::failure("SPOOL_COMMIT_FAILED"))?;
    let size = file.byte_len();
    let digest = payload::hash(&file.bytes);
    Ok(Payload::File {
        key,
        name: file.name,
        mime: file.mime_type,
        size,
        digest,
    })
}

async fn check_capacity(runtime: &Runtime, required: u64) -> Result<(), ChatChannelError> {
    let mut entries = tokio::fs::read_dir(&runtime.spool)
        .await
        .map_err(|_| payload::failure("SPOOL_UNAVAILABLE"))?;
    let mut total = required;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|_| payload::failure("SPOOL_UNAVAILABLE"))?
    {
        total = total.saturating_add(
            entry
                .metadata()
                .await
                .map_err(|_| payload::failure("SPOOL_UNAVAILABLE"))?
                .len(),
        );
        if total > MAX_SPOOL_BYTES {
            return Err(payload::failure("SPOOL_CAPACITY_REACHED"));
        }
    }
    Ok(())
}

fn path(runtime: &Runtime, key: &str) -> Result<PathBuf, ChatChannelError> {
    uuid::Uuid::parse_str(key).map_err(|_| payload::failure("INVALID_SPOOL_KEY"))?;
    Ok(runtime.spool.join(format!("{key}.bin")))
}

pub(super) async fn file(
    runtime: &Runtime,
    stored: &Payload,
) -> Result<ChannelAttachment, ChatChannelError> {
    let Payload::File {
        key,
        name,
        mime,
        size,
        digest,
    } = stored
    else {
        return Err(payload::failure("INVALID_FILE_DELIVERY"));
    };
    let path = path(runtime, key)?;
    let metadata = tokio::fs::symlink_metadata(&path)
        .await
        .map_err(|_| payload::failure("SPOOL_FILE_MISSING"))?;
    if !metadata.is_file() || metadata.len() != *size || *size > MAX_INBOUND_BYTES as u64 {
        return Err(payload::failure("SPOOL_FILE_CHANGED"));
    }
    use tokio::io::AsyncReadExt;
    let mut bytes = Vec::new();
    tokio::fs::File::open(path)
        .await
        .map_err(|_| payload::failure("SPOOL_FILE_MISSING"))?
        .take(size.saturating_add(1))
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| payload::failure("SPOOL_READ_FAILED"))?;
    if bytes.len() as u64 != *size || payload::hash(&bytes) != *digest {
        return Err(payload::failure("SPOOL_FILE_CHANGED"));
    }
    Ok(ChannelAttachment {
        name: name.clone(),
        mime_type: mime.clone(),
        bytes: Arc::from(bytes),
    })
}

pub(super) async fn cleanup(runtime: &Runtime, envelope: &Envelope) {
    if let Payload::File { key, .. } = &envelope.payload {
        if let Ok(path) = path(runtime, key) {
            if let Err(error) = tokio::fs::remove_file(path).await {
                tracing::warn!(category = ?error.kind(), "[channel-delivery] spool cleanup failed");
            }
        }
    }
}
