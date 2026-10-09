// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// How far the wall clock may step back under a grant (a time sync, say)
/// before the grant is ended rather than trusted to still be fresh.
pub const MAX_CLOCK_SKEW_MS: i64 = 60_000;

/// The level a window is at, reading the absence of a grant as `None`.
pub fn level_of(grant: Option<&ComputerGrant>) -> GrantLevel {
    grant.map_or(GrantLevel::None, |g| g.level)
}

/// A window's title as an agent may see it: only from [`GrantLevel::Read`]
/// upwards, and never as an empty string (the platform withholds titles it
/// may not show, and "" would read as "this window has no title").
pub fn visible_title(level: GrantLevel, title: &str) -> Option<String> {
    (level.allows(GrantLevel::Read) && !title.is_empty()).then(|| title.to_string())
}

/// Names one read of one grant: `<epoch>.<read number>`. The epoch moves every
/// time the grant on the window changes hands (shared, revoked, re-shared), so
/// a generation from before a revoke never names a read made after it.
pub fn generation(epoch: u64, seq: u64) -> String {
    format!("{epoch}.{seq}")
}

/// Why a window cannot be shared at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NotGrantable {
    /// One of iyw-claw's own windows. Not a setting: an agent that could read
    /// iyw-claw's windows could read the grant dialog and the stop button, and
    /// the only thing keeping a person in charge of what an agent may see is
    /// that those are out of its reach.
    OwnApp,
    /// The application is on the person's never-share list — one of the
    /// defaults they kept (credential managers, the system settings), or one
    /// they added.
    Blocklisted,
    /// iyw-claw cannot tell which application this is, or which run of it: the
    /// platform gave no start time for its process (so a later process under
    /// the same pid could not be told apart), or nothing a blocklist could
    /// match (no bundle identifier, no path).
    Unidentified,
}
