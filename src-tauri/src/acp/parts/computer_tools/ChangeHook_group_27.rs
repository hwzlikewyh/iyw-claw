// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// See [`ComputerToolsRuntimeConfig::on_change`].
pub(super) type ChangeHook = Box<dyn Fn(&ComputerToolsConfig, &ComputerToolsConfig) + Send + Sync>;
