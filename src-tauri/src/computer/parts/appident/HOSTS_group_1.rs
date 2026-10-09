// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Windows: the executables whose windows are other applications'. See the
/// module note.
pub(super) const HOSTS: &[&str] = &[FRAME_HOST, "msedgewebview2.exe"];

/// Windows: the host whose every window is a frame with an application
/// drawing inside it. See the module note.
pub(super) const FRAME_HOST: &str = "ApplicationFrameHost.exe";

/// Windows: the folder the system keeps its own agents in. See the module
/// note.
pub(super) const SYSTEM_APPS: &str = "SystemApps";
