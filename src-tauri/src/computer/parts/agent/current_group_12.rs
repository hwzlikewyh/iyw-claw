// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl SelfIdentity {
    pub fn current() -> Self {
        let exe = std::env::current_exe().ok();
        let bundle = exe.as_deref().and_then(enclosing_app_bundle);
        Self {
            pid: std::process::id(),
            exe,
            bundle,
        }
    }

    /// Whether `app` is iyw-claw: this process, any process calling itself
    /// iyw-claw's bundle, any executable named as iyw-claw's is, or anything run
    /// from this iyw-claw's executable or bundle.
    pub fn owns(&self, app: &RawApp) -> bool {
        if app.pid == self.pid {
            return true;
        }
        if app
            .bundle_id
            .as_deref()
            .is_some_and(|b| b.eq_ignore_ascii_case(IYW_CLAW_BUNDLE_ID))
        {
            return true;
        }
        let Some(path) = app.path.as_deref().filter(|s| !s.is_empty()) else {
            return false;
        };
        // Split on both separators, as the blocklist does: a Windows path is
        // still one when checked in a test on another platform.
        if path
            .rsplit(['/', '\\'])
            .find(|part| !part.is_empty())
            .is_some_and(|file| {
                IYW_CLAW_EXECUTABLES
                    .iter()
                    .any(|name| file.eq_ignore_ascii_case(name))
            })
        {
            return true;
        }
        let path = Path::new(path);
        [self.exe.as_deref(), self.bundle.as_deref()]
            .into_iter()
            .flatten()
            .any(|mine| same_path(mine, path))
    }

    /// Whether any word of a launch command is iyw-claw: its bundle
    /// identifier, an executable of its name, or this very executable or
    /// bundle.
    pub fn owns_command(&self, command: &str) -> bool {
        command_words(command).iter().any(|word| {
            self.owns(&RawApp {
                pid: 0,
                name: String::new(),
                bundle_id: Some(word.clone()),
                path: Some(word.clone()),
                active: false,
                started_at: None,
            })
        })
    }
}
