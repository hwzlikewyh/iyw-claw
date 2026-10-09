// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl SharingPolicy {
    pub(in crate::commands::computer) fn of(config: &ComputerToolsConfig) -> Self {
        Self {
            enabled: config.enabled,
            screen_enabled: config.screen_enabled,
            blocklist: blocklist_of(config),
        }
    }
}
