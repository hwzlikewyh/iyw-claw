use std::path::PathBuf;

use serde_json::{json, Value};

pub(super) struct ThreadWorkspace {
    pub cwd: PathBuf,
    pub roots: Vec<PathBuf>,
}

impl ThreadWorkspace {
    pub(super) fn apply(&self, request: &mut Value) {
        // 上游每次启动线程会重载配置，不能依赖进程 cwd 或启动时的 Config。
        request["params"]["cwd"] = json!(self.cwd);
        request["params"]["runtimeWorkspaceRoots"] = json!(self.roots);
    }
}
