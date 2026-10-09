// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// What sharing several windows at once did.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareManyResult {
    pub shared: Vec<SharedWindow>,
    /// How many of the windows named were not shared: closed since the list
    /// was read, or never shareable.
    pub skipped: u32,
}

/// Share every window named at one level — the picker's "all" — each exactly
/// as [`computer_share_window_core`] would share it, one after another,
/// skipping the ones that cannot be. A Stop or a switch-off that lands part
/// way through is decided window by window, under the lock it revokes under:
/// nothing is shared after it (the next share, begun after the Stop, is).
/// Refused outright when the first window already could not be shared for
/// that reason.
pub async fn computer_share_windows_core(
    service: &ComputerService,
    target_ids: &[String],
    level: GrantLevel,
) -> Result<ShareManyResult, AppCommandError> {
    let since = service.stop_count();
    let mut skipped = 0u32;
    for (i, target_id) in target_ids.iter().enumerate() {
        match service.share_unless_stopped(target_id, level, since) {
            // Told as it happens, so a Stop's revocations are never told
            // before a share they undid.
            Ok(change) => service.announce(&change.into_iter().collect::<Vec<_>>()),
            Err(e) if i == 0 && level != GrantLevel::None && !service.sharing_open(since) => {
                return Err(e)
            }
            Err(_) => skipped += 1,
        }
    }
    Ok(ShareManyResult {
        shared: service.targets.shared(),
        skipped,
    })
}
