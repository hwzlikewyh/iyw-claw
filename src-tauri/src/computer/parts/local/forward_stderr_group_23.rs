// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The helper's stderr (and, through it, the driver's) into iyw-claw's log.
pub(super) async fn forward_stderr(stderr: Box<dyn AsyncRead + Send + Unpin>) {
    let mut lines = BufReader::new(stderr).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        tracing::info!(target: "computer_helper", "{line}");
    }
}
