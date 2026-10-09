// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Enough about this iyw-claw process to recognise its windows in a listing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfIdentity {
    pub pid: u32,
    /// The running executable.
    pub exe: Option<PathBuf>,
    /// The `.app` bundle the executable is in, on macOS — the path the
    /// platform reports for an application.
    pub bundle: Option<PathBuf>,
}
