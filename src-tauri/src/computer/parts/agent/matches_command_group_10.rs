// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl Blocklist {
    /// Whether any word of a launch command names an application on the
    /// list — as a whole, or by the file name at the end of a path — the
    /// executable, an argument, a wrapper's application id. Erring towards
    /// refusing: a command that so much as mentions one is not started.
    pub fn matches_command(&self, command: &str) -> bool {
        command_words(command).iter().any(|word| {
            let file = word.rsplit(['/', '\\']).find(|part| !part.is_empty());
            [Some(word.as_str()), file]
                .into_iter()
                .flatten()
                .map(str::to_lowercase)
                .any(|name| self.entries.binary_search(&name).is_ok())
        })
    }
}
