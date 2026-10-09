// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Whether `bundle` is named `<name>.app`.
pub fn has_app_extension(bundle: &str) -> bool {
    let name = bundle.rsplit('/').next().unwrap_or(bundle);
    strip_suffix_ignore_case(name, ".app").len() < name.len()
}

/// Whether `bundle` is an application: by its name, or by the package type
/// its `Info.plist` gives (`package_type`).
pub fn is_application(bundle: &str, package_type: Option<&str>) -> bool {
    has_app_extension(bundle) || package_type == Some("APPL")
}

/// The bundle whose main executable `executable` is, or `None` when it is
/// anything else: an executable nested deeper in a bundle, or one outside any.
/// Whether that bundle is an application is [`is_application`]'s question.
pub fn executable_bundle(executable: &str) -> Option<&str> {
    let (dir, file) = executable.rsplit_once('/')?;
    if file.is_empty() {
        return None;
    }
    let bundle = dir.strip_suffix("/Contents/MacOS")?;
    let name = bundle.rsplit('/').next()?;
    (!name.is_empty()).then_some(bundle)
}

/// The outermost `.app` bundle on `bundle`'s path — `bundle` itself unless it
/// sits inside another application. See the module note.
pub fn outermost_app_bundle(bundle: &str) -> &str {
    let mut end = 0;
    for part in bundle.split('/') {
        end += part.len();
        if has_app_extension(part) {
            return &bundle[..end];
        }
        end += 1;
    }
    bundle
}

/// Whether `bundle` is one of Apple's own agents or panels rather than an
/// application a person uses. See the module note.
pub fn is_system_component(bundle: &str) -> bool {
    bundle.starts_with("/System/")
        && !SYSTEM_APPLICATIONS
            .iter()
            .any(|dir| bundle.starts_with(dir))
        && !bundle.eq_ignore_ascii_case(FINDER)
}

/// Whether the executable at `path` is an application a person uses, as
/// Windows runs them: not a host, whose windows are other applications', and
/// not one of the system's own agents (see the module note). Hosts are known
/// by their file names, in any case, wherever they are; the agents by where
/// they are, the `SystemApps` folder of the system's own Windows folder — or,
/// where the system will not say which folder that is, any folder named so.
/// Both can only keep a window from being shared, never let one be.
pub fn is_windows_application(path: &str) -> bool {
    is_application_outside(path, system_apps_folder())
}
