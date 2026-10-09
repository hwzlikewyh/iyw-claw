// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Whether exactly one app bundle is around `exe`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(super) fn alone_in_its_app(exe: &Path) -> bool {
    exe.ancestors()
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("app")))
        .count()
        == 1
}

/// The helper app `helper` is the main executable of, when that app sits
/// inside another app's bundle — where macOS would charge its Screen
/// Recording to the app around it.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(super) fn nested_helper_app(helper: &Path) -> Option<&Path> {
    let macos = helper.parent()?;
    let contents = macos.parent()?;
    let app = contents.parent()?;
    let named = |p: &Path, name: &str| p.file_name().is_some_and(|n| n == name);
    let inside_an_app = app
        .ancestors()
        .skip(1)
        .any(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("app")));
    (named(macos, "MacOS")
        && named(contents, "Contents")
        && named(app, HELPER_APP)
        && inside_an_app)
        .then_some(app)
}
