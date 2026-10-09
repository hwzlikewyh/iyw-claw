// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The array `key` of a successful answer. Missing is a malformed answer, not
/// an empty one: an application list read as empty would leave every window
/// without the application that names it.
pub(super) fn required_array<'a>(
    tool: &str,
    value: &'a Value,
    key: &str,
) -> Result<&'a [Value], HelperError> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| HelperError::failed(format!("{tool} answered without `{key}`")))
}
