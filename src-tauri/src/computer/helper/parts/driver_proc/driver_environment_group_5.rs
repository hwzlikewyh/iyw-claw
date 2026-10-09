// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The driver's whole environment for one launch. See the module note.
pub fn driver_environment(run_dir: &Path) -> Vec<(String, String)> {
    driver_environment_with(run_dir, |key| std::env::var(key).ok())
}
