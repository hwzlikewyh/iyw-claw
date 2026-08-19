use std::path::Path;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::acp::capability_policy::CapabilityRevocationMonitor;
use crate::app_error::AppCommandError;

const COPY_BUFFER_BYTES: usize = 64 * 1024;

pub(super) async fn copy_attachment(
    source: &Path,
    destination: &Path,
    monitor: &CapabilityRevocationMonitor,
) -> Result<(), AppCommandError> {
    let mut input = tokio::fs::File::open(source)
        .await
        .map_err(AppCommandError::io)?;
    let mut output = tokio::fs::File::create(destination)
        .await
        .map_err(AppCommandError::io)?;
    let mut buffer = vec![0_u8; COPY_BUFFER_BYTES];
    loop {
        let read = monitor
            .run_until_revoked(input.read(&mut buffer))
            .await?
            .map_err(AppCommandError::io)?;
        if read == 0 {
            break;
        }
        monitor
            .run_until_revoked(output.write_all(&buffer[..read]))
            .await?
            .map_err(AppCommandError::io)?;
    }
    monitor
        .run_until_revoked(output.flush())
        .await?
        .map_err(AppCommandError::io)?;
    monitor.require_current().await
}
