// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// A `tools/call` result.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToolCallResult {
    pub is_error: bool,
    pub content: Vec<Value>,
    pub structured: Option<Value>,
}
