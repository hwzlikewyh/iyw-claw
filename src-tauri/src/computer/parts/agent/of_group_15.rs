// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl ComputerAction {
    /// The line an action request leaves.
    pub fn of(request: &crate::computer::types::ComputerActRequest) -> Self {
        use crate::computer::types::ComputerActRequest as R;
        match request {
            R::Click { .. } => ComputerAction::Click,
            R::Drag { .. } => ComputerAction::Drag,
            R::Scroll { .. } => ComputerAction::Scroll,
            R::Type { .. } => ComputerAction::Type,
            R::Key { .. } => ComputerAction::Key,
            R::HoldKey { .. } => ComputerAction::HoldKey,
            R::SetValue { .. } => ComputerAction::SetValue,
            R::Restore => ComputerAction::Restore,
            R::InvokeMenu { .. } => ComputerAction::Menu,
            R::SetFrame { .. } => ComputerAction::SetFrame,
        }
    }
}
