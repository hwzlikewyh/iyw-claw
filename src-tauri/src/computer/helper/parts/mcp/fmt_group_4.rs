// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl std::fmt::Display for McpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McpError::Closed => f.write_str("the driver closed its connection"),
            McpError::Timeout => f.write_str("the driver did not answer in time"),
            McpError::Rpc { code, message } => {
                write!(f, "the driver refused the call ({code}): {message}")
            }
            McpError::Protocol(why) => {
                write!(f, "the driver answered in an unexpected shape: {why}")
            }
        }
    }
}
