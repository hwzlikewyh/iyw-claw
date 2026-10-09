// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// What `computer_shared_state` answers: what `computer://state` carries.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedState {
    pub shared: Vec<SharedWindow>,
    /// The applications shared as a whole.
    pub apps: Vec<SharedApp>,
    /// The entire screen, when it is shared.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screen: Option<SharedScreen>,
}

/// Share the entire screen at `level`, or end its share at `none`. Answers
/// with what is shared now.
pub async fn computer_share_screen_core(
    service: &ComputerService,
    level: GrantLevel,
) -> Result<SharedState, AppCommandError> {
    let since = service.stop_count();
    let change = service.share_screen_unless_stopped(level, since)?;
    service.announce_change(change);
    Ok(service.shared_state())
}
