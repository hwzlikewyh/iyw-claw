use std::sync::Arc;

use super::{error::GatewayError, Runtime};

// permit 随阻塞任务释放：HTTP 取消也不能让后台扫描/解码突破并发上限。
pub(super) async fn blocking<T, F>(runtime: &Runtime, operation: F) -> Result<T, GatewayError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, GatewayError> + Send + 'static,
{
    let permit = Arc::clone(&runtime.work)
        .acquire_owned()
        .await
        .map_err(|_| GatewayError::unavailable("Local preparation queue closed"))?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        operation()
    })
    .await
    .map_err(|_| GatewayError::unavailable("Local preparation task failed"))?
}
