// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl PropVariant {
    pub(in crate::computer::helper::hwnd) fn empty() -> Self {
        Self {
            kind: 0,
            reserved: [0; 3],
            value: [0; 2],
        }
    }

    /// The string the value holds, when it holds one no longer than an
    /// application user model id can be.
    pub(in crate::computer::helper::hwnd) fn app_user_model_id(&self) -> Option<String> {
        if self.kind != VT_LPWSTR || self.value[0] == 0 {
            return None;
        }
        let text = self.value[0] as *const u16;
        // SAFETY: a VT_LPWSTR value points at a NUL-terminated string the
        // value owns until it is cleared; it is read up to its NUL, and no
        // further than the longest id there is.
        let len = (0..MAX_APP_USER_MODEL_ID).find(|i| unsafe { *text.add(*i) } == 0)?;
        // SAFETY: the `len` units just read, before the NUL.
        let units = unsafe { std::slice::from_raw_parts(text, len) };
        String::from_utf16(units).ok().filter(|id| !id.is_empty())
    }
}
