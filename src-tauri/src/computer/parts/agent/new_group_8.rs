// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl Blocklist {
    /// Every default entry, plus `user_entries`.
    pub fn new(user_entries: &[String]) -> Self {
        Self::configured(user_entries, &[])
    }

    /// The list as it stands, lowercased: what it is sent to the helper as.
    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    /// A list made of `entries` as [`entries`](Self::entries) gave them.
    pub fn from_entries(entries: &[String]) -> Self {
        let mut entries: Vec<String> = entries.iter().map(|e| e.to_lowercase()).collect();
        entries.sort();
        entries.dedup();
        Self { entries }
    }

    /// The default entries but those whose keys are in `removed`, plus
    /// `user_entries`.
    pub fn configured(user_entries: &[String], removed: &[String]) -> Self {
        let mut entries: Vec<String> = DEFAULT_BLOCKLIST
            .iter()
            .filter(|block| !removed.iter().any(|key| key == block.key))
            .flat_map(DefaultBlock::all_names)
            .map(str::to_lowercase)
            .chain(
                user_entries
                    .iter()
                    .map(|s| s.trim().to_lowercase())
                    .filter(|s| !s.is_empty()),
            )
            .collect();
        entries.sort();
        entries.dedup();
        Self { entries }
    }

    /// Whether `app` is on the list, by bundle identifier, by full path, or by
    /// the file name at the end of its path — for a Chromium browser running
    /// from its clone (`Foo.app.bundle`, see `appident`), also by the file
    /// name of the bundle it was cloned from (`Foo.app`).
    pub fn matches(&self, app: &RawApp) -> bool {
        let mut names: Vec<String> = Vec::with_capacity(4);
        if let Some(bundle) = app.bundle_id.as_deref().filter(|s| !s.is_empty()) {
            names.push(bundle.to_lowercase());
        }
        if let Some(path) = app.path.as_deref().filter(|s| !s.is_empty()) {
            names.push(path.to_lowercase());
            // Split on both separators, not `Path::file_name`: a Windows path
            // is still a Windows path when the list is checked in a test on
            // another platform, and a path from the driver is only ever one
            // platform's spelling anyway.
            if let Some(file) = path.rsplit(['/', '\\']).find(|part| !part.is_empty()) {
                let file = file.to_lowercase();
                if let Some(cloned) = file.strip_suffix(".bundle").filter(|f| f.ends_with(".app")) {
                    names.push(cloned.to_string());
                }
                names.push(file);
            }
        }
        names
            .iter()
            .any(|name| self.entries.binary_search(name).is_ok())
    }
}
