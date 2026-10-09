// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Windows: an application, as the process running it shows.
#[cfg(windows)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsApp {
    /// Its executable's full path, which is what it is known by.
    pub path: String,
    /// What to call it (see the module note).
    pub name: String,
}

/// Windows: whose the windows of a process are (see the module note).
#[cfg(windows)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowsOwner {
    /// An application ([`is_windows_application`]): they are its own.
    Application(WindowsApp),
    /// The frame host: each is the application drawing inside it.
    FrameHost,
    /// Another host, one of the system's agents, or a process that has gone
    /// or will not say: nobody's that can be told.
    Unknown,
}

/// Windows: whose the windows of `pid` are — read off the process, while it
/// is still the one that started at `started_at`.
#[cfg(windows)]
pub fn windows_owner(pid: u32, started_at: u64) -> WindowsOwner {
    let Some(image) =
        crate::computer::procinfo::process_image(pid).filter(|image| image.started == started_at)
    else {
        return WindowsOwner::Unknown;
    };
    if is_frame_host(&image.path) {
        return WindowsOwner::FrameHost;
    }
    if !is_windows_application(&image.path) {
        return WindowsOwner::Unknown;
    }
    WindowsOwner::Application(WindowsApp {
        name: windows_name(&image.path, image.app_user_model_id.as_deref()),
        path: image.path,
    })
}

/// Windows: the application `pid` runs — when the process is still the one
/// that started at `started_at`, and runs an application
/// ([`is_windows_application`]).
#[cfg(windows)]
pub fn windows_application(pid: u32, started_at: u64) -> Option<WindowsApp> {
    match windows_owner(pid, started_at) {
        WindowsOwner::Application(app) => Some(app),
        WindowsOwner::FrameHost | WindowsOwner::Unknown => None,
    }
}
