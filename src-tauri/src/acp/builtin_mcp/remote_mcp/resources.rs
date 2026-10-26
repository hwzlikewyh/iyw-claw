use std::sync::{Arc, OnceLock};

use rmcp::ErrorData;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use super::failure;

const DIRECTORY_IN_FLIGHT: usize = 128;
const BUSINESS_IN_FLIGHT: usize = 128;

pub(super) fn admit(execution: bool) -> Result<OwnedSemaphorePermit, ErrorData> {
    static DIRECTORY: OnceLock<Arc<Semaphore>> = OnceLock::new();
    static BUSINESS: OnceLock<Arc<Semaphore>> = OnceLock::new();
    let permits = if execution {
        BUSINESS.get_or_init(|| Arc::new(Semaphore::new(BUSINESS_IN_FLIGHT)))
    } else {
        DIRECTORY.get_or_init(|| Arc::new(Semaphore::new(DIRECTORY_IN_FLIGHT)))
    };
    permits.clone().try_acquire_owned().map_err(|_| {
        failure(
            "remote_at_capacity",
            "Remote MCP request capacity reached; no request was dispatched",
            false,
        )
    })
}
