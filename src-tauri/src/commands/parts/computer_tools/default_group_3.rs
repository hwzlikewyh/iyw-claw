// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl Default for ComputerToolsSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            grant_ttl_minutes: DEFAULT_GRANT_TTL_MINUTES,
            blocklist: Vec::new(),
            blocklist_removed: Vec::new(),
            blocklist_defaults: default_blocklist(Platform::current()),
            stop_shortcut: default_stop_shortcut(),
            show_indicator: default_show_indicator(),
            allow_foreground: default_allow_foreground(),
            default_delivery: ActDelivery::Background,
            launch_enabled: false,
            clipboard_enabled: false,
            screen_enabled: false,
        }
    }
}

impl ComputerToolsSettings {
    pub(in crate::commands::computer_tools) fn into_runtime_config(self) -> ComputerToolsConfig {
        ComputerToolsConfig {
            enabled: self.enabled,
            grant_ttl: (self.grant_ttl_minutes > 0)
                .then(|| Duration::from_secs(u64::from(self.grant_ttl_minutes) * 60)),
            blocklist: normalize_blocklist(self.blocklist),
            blocklist_removed: normalize_removed(self.blocklist_removed),
            stop_shortcut: StopShortcut::from_setting(&self.stop_shortcut, Platform::current()),
            show_indicator: self.show_indicator,
            allow_foreground: self.allow_foreground,
            default_delivery: self.default_delivery,
            launch_enabled: self.launch_enabled,
            clipboard_enabled: self.clipboard_enabled,
            screen_enabled: self.screen_enabled,
            // Kept by the runtime handle, not by the record.
            switched_off: 0,
        }
    }
}
