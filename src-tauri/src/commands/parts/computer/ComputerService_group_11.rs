// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The computer-use service: one per process — the desktop app's, managed as
/// Tauri state, or iyw-claw-server's where it is let share the screen it runs
/// on (`IYW_CLAW_COMPUTER_USE`).
pub struct ComputerService {
    /// Where what changes is told (see `computer::events`).
    pub(in crate::commands::computer) events: ComputerEvents,
    /// Where a settings change the service makes itself is told — removing
    /// the driver switches computer use off.
    pub(in crate::commands::computer) settings_events: EventEmitter,
    /// What only the desktop app has; `None` in iyw-claw-server.
    #[cfg(feature = "tauri-runtime")]
    pub(in crate::commands::computer) desktop: Option<DesktopUi>,
    pub(in crate::commands::computer) backend: Arc<LocalBackend>,
    pub(in crate::commands::computer) targets: TargetTable,
    pub(in crate::commands::computer) config: ComputerToolsRuntimeConfig,
    pub(in crate::commands::computer) me: SelfIdentity,
    /// Held for every call that reaches the driver. See the module note.
    pub(in crate::commands::computer) turn: tokio::sync::Mutex<()>,
    /// How many times the person has pressed Stop. Whatever began before
    /// the latest one — a share, a read, an action — is refused when it
    /// finds the count moved; whatever begins after it goes on as usual.
    pub(in crate::commands::computer) stops: AtomicU64,
    /// Held across "has a Stop come since this share began?" and the share
    /// that follows, and across a Stop's count and the revocation that
    /// follows — so a share begun before a Stop cannot slip in between the
    /// two and outlive it. The same holds for a settings change and what it
    /// takes away: see `policy`.
    pub(in crate::commands::computer) grant_gate: std::sync::Mutex<()>,
    /// The switch and the blocklist as the last settings change left them,
    /// written under `grant_gate` by the change hook, which revokes under it
    /// too. A share decides by these, under the same lock: it lands either
    /// before a switch-off (and is revoked with the rest) or after it (and is
    /// refused) — never after the revocation and still standing.
    pub(in crate::commands::computer) policy: std::sync::Mutex<SharingPolicy>,
    /// Whether the person wants the strip at all (Settings), as the last
    /// settings change the service followed left it.
    pub(in crate::commands::computer) strip_wanted: AtomicBool,
    /// Held across reading the state and telling everyone of it, so two
    /// changes told at once are told in the order they were read — the
    /// older never lands last.
    pub(in crate::commands::computer) state_gate: std::sync::Mutex<()>,
    /// The driver as Settings manages it.
    pub(in crate::commands::computer) drivers: Arc<DriverAdmin>,
    /// What an agent last put on the clipboard itself (see
    /// [`OwnedClipboard`]); cleared by Stop.
    pub(in crate::commands::computer) clipboard: std::sync::Mutex<Option<OwnedClipboard>>,
}
