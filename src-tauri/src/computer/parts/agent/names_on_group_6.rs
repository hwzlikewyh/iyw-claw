// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl DefaultBlock {
    pub(in crate::computer::agent) fn names_on(
        &self,
        platform: Platform,
    ) -> &'static [&'static str] {
        match platform {
            Platform::Mac => self.macos,
            Platform::Windows => self.windows,
            Platform::Linux => self.linux,
        }
    }

    pub(in crate::computer::agent) fn all_names(&self) -> impl Iterator<Item = &'static str> {
        self.macos
            .iter()
            .chain(self.windows)
            .chain(self.linux)
            .copied()
    }
}
