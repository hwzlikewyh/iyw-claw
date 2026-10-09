// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl ComputerActRequest {
    /// Whether the action is one the person allows only with "Let agents
    /// open applications and move their windows" on: it changes where the
    /// person's own windows are.
    pub fn needs_launch_switch(&self) -> bool {
        matches!(self, Self::SetFrame { .. })
    }

    /// Whether the window can be brought to the front for this action
    /// ([`ActDelivery::Foreground`]). A value is set through the
    /// application's accessibility interface, which no window has to be in
    /// front for, and a restore is iyw-claw's own call: those two only ever go
    /// in the background.
    pub fn can_come_forward(&self) -> bool {
        matches!(
            self,
            Self::Click { .. }
                | Self::Drag { .. }
                | Self::Scroll { .. }
                | Self::Type { .. }
                | Self::Key { .. }
                | Self::HoldKey { .. }
        )
    }

    /// Whether the action can be done at all only by bringing the window to
    /// the front on `platform`: restoring a window on Linux, where the only
    /// way back is the window manager's activation; and a menu command,
    /// which the drivers choose with the application active.
    pub fn needs_front(&self, platform: crate::computer::keys::Platform) -> bool {
        match self {
            Self::Restore => platform == crate::computer::keys::Platform::Linux,
            Self::InvokeMenu { .. } => true,
            _ => false,
        }
    }
}
