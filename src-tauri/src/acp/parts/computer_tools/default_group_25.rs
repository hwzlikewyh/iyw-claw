// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl Default for ComputerToolsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            grant_ttl: None,
            blocklist: Vec::new(),
            blocklist_removed: Vec::new(),
            stop_shortcut: None,
            show_indicator: true,
            allow_foreground: true,
            default_delivery: ActDelivery::Background,
            launch_enabled: false,
            clipboard_enabled: false,
            screen_enabled: false,
            switched_off: 0,
        }
    }
}

impl ComputerToolsConfig {
    /// What an action gets when the agent does not ask: the person's choice
    /// while they allow the front at all, the background otherwise.
    pub fn default_delivery_in_force(&self) -> ActDelivery {
        if self.allow_foreground {
            self.default_delivery
        } else {
            ActDelivery::Background
        }
    }
}
