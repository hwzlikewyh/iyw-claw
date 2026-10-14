pub(in crate::acp::builtin_mcp) const MAX_BATCH_ITEMS: usize = 64;
pub(in crate::acp::builtin_mcp) const MAX_BATCH_EXECUTIONS: usize = 128;
pub(in crate::acp::builtin_mcp) const MAX_CONCURRENT_EXECUTIONS: usize = 16;
pub(in crate::acp::builtin_mcp) const MAX_REQUEST_COUNT: usize = 4;
pub(in crate::acp::builtin_mcp) const MAX_FALLBACKS: usize = 3;
pub(in crate::acp::builtin_mcp) const MAX_IMAGE_INPUTS: usize = 17;
pub(in crate::acp::builtin_mcp) const OUTPUT_RETRY_DELAYS_SECONDS: [u64; 3] = [2, 4, 8];
pub(in crate::acp::builtin_mcp) const MAX_BATCH_ATTEMPTS: usize =
    MAX_BATCH_EXECUTIONS * (OUTPUT_RETRY_DELAYS_SECONDS.len() + 1);
