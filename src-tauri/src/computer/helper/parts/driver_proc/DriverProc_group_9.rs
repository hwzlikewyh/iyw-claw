// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// A running, verified driver.
pub struct DriverProc {
    pub(in crate::computer::helper::driver_proc) client: Arc<McpClient>,
    pub(in crate::computer::helper::driver_proc) child: ChildProc,
    pub(in crate::computer::helper::driver_proc) run_dir: PathBuf,
    /// The driver said it captures windows at their own size (see the module
    /// note). Without it a capture's pixels cannot be mapped back to the
    /// window's, and nothing may be pointed at by coordinates.
    pub(in crate::computer::helper::driver_proc) full_size_captures: bool,
}
