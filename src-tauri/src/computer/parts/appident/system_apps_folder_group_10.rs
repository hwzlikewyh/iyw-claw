// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Windows: the `SystemApps` folder of the system's own Windows folder,
/// asked once; `None` when the system will not say.
#[cfg(windows)]
pub(super) fn system_apps_folder() -> Option<&'static str> {
    use std::sync::OnceLock;

    // Declared here: windows-sys has it behind a feature this crate does not
    // turn on (`Win32_System_SystemInformation`), and turning one on rebuilds
    // every crate that shares windows-sys — Tauri among them.
    #[link(name = "kernel32")]
    extern "system" {
        fn GetSystemWindowsDirectoryW(buffer: *mut u16, size: u32) -> u32;
    }
    static FOLDER: OnceLock<Option<String>> = OnceLock::new();
    FOLDER
        .get_or_init(|| {
            let mut buf = [0u16; 512];
            // SAFETY: `buf` holds as many units as said; the answer is the
            // number written without the NUL, or the size needed when that
            // is more than there is room for.
            let len = unsafe { GetSystemWindowsDirectoryW(buf.as_mut_ptr(), buf.len() as u32) };
            let len = usize::try_from(len)
                .ok()
                .filter(|len| (1..buf.len()).contains(len))?;
            let windows = String::from_utf16(&buf[..len]).ok()?;
            Some(format!(r"{}\{SYSTEM_APPS}", windows.trim_end_matches('\\')))
        })
        .as_deref()
}
