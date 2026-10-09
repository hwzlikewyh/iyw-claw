// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// What to show in the Finder for adding the helper to System Settings by
/// hand: the helper app iyw-claw runs, where there is one — the executable
/// inside it would be listed by its path, which macOS never asks about — and
/// the helper itself otherwise.
pub async fn helper_to_reveal() -> Result<PathBuf, BackendError> {
    Ok(app_of(helper_to_run().await?))
}
