// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl AppIdentity {
    /// What to call the application, given the name the window list gives the
    /// process that owns a window (`owner`). See the module note.
    pub fn name(&self, owner: &str) -> String {
        let owner = owner.trim();
        // A helper's own name is not the application's.
        if self.nested || owner.is_empty() {
            return file_name(&self.path);
        }
        // A bundle not named `.app` is a Chromium clone, named after the
        // installed bundle rather than by it.
        if !has_app_extension(&self.path) || !self.plist_names.iter().any(|n| n == owner) {
            return owner.to_string();
        }
        file_name(&self.path)
    }
}
