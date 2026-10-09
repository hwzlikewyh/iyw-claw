// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The `computer_*` tools' access impl: the service, from the listener.
pub struct McpComputerTools {
    pub(in crate::commands::computer) service: Arc<ComputerService>,
}
