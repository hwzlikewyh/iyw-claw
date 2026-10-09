// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpError {
    /// The driver closed its side, or exited.
    Closed,
    Timeout,
    /// The driver answered with a JSON-RPC error.
    Rpc {
        code: i64,
        message: String,
    },
    /// The driver answered with something that is not a JSON-RPC reply.
    Protocol(String),
}
