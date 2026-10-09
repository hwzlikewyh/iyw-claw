// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// [`is_windows_application`], with the system's agents in `system_apps` —
/// or, where that is not known, in any folder named `SystemApps`.
pub(super) fn is_application_outside(path: &str, system_apps: Option<&str>) -> bool {
    let mut parts = path.rsplit(['\\', '/']);
    let Some(file) = parts.next().filter(|file| !file.is_empty()) else {
        return false;
    };
    if HOSTS.iter().any(|host| file.eq_ignore_ascii_case(host)) {
        return false;
    }
    match system_apps {
        Some(folder) => !is_inside(path, folder),
        None => !parts.any(|dir| dir.eq_ignore_ascii_case(SYSTEM_APPS)),
    }
}

/// Whether `path` lies inside `folder`: the same letters, in any case, with
/// either separator, and a separator after them.
pub(super) fn is_inside(path: &str, folder: &str) -> bool {
    let fold = |c: char| {
        if c == '/' {
            '\\'
        } else {
            c.to_ascii_lowercase()
        }
    };
    let mut path = path.chars().map(fold);
    folder.chars().map(fold).all(|c| path.next() == Some(c)) && path.next() == Some('\\')
}
