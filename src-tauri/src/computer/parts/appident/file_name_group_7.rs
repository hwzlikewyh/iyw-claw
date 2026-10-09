// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The bundle's file name, as the Finder shows it: without `.app` — or, for a
/// Chromium clone, without `.app.bundle`.
pub(super) fn file_name(bundle: &str) -> String {
    let file = bundle.rsplit('/').next().unwrap_or(bundle);
    let file = strip_suffix_ignore_case(file, ".bundle");
    strip_suffix_ignore_case(file, ".app").to_string()
}

/// `name` without `suffix` (ASCII, in any case), unless nothing would be left.
pub(super) fn strip_suffix_ignore_case<'a>(name: &'a str, suffix: &str) -> &'a str {
    // `suffix` is ASCII, so where the name ends with it the bytes before it
    // end on a character boundary; anywhere else `get` says no.
    match name.len().checked_sub(suffix.len()).filter(|n| *n > 0) {
        Some(stem)
            if name
                .get(stem..)
                .is_some_and(|end| end.eq_ignore_ascii_case(suffix)) =>
        {
            &name[..stem]
        }
        _ => name,
    }
}
