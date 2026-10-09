// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// How long one request may take to go out. A helper that stops reading is
/// wedged, and half a frame on the socket cannot be taken back.
pub(super) const WRITE_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a freshly launched helper has to say it is ready.
pub(super) const READY_TIMEOUT: Duration = Duration::from_secs(15);

/// Why a helper that never said it was ready failed. On macOS a helper
/// macOS has not seen before can be held at launch by a question of the
/// system's own — whether it may read from a removable volume, for one, when
/// iyw-claw lives on another disk — until the person answers it.
pub(super) const NOT_STARTED: &str = if cfg!(target_os = "macos") {
    "the helper did not start in time — if macOS is asking about \
     iyw-computer-helper, answer it and try again"
} else {
    "the helper did not start in time"
};

/// How long a helper that has been told iyw-claw is done gets to exit on its
/// own: it stops its driver first — one still starting once it has started —
/// within its own bound (`helper::SHUTDOWN_GRACE`, 60 s). Past this it is
/// stopped by signal.
pub(super) const HELPER_EXIT_GRACE: Duration = Duration::from_secs(65);

/// How long [`LocalBackend::close_now`] waits for the helper to say its
/// driver is gone. A driver that is starting when the `Halt` arrives is
/// stopped as soon as it has started; the helper bounds a start by its
/// permission check (10 s), the driver's handshake (20 s) and its
/// configuration (10 s), and a stop by a couple of seconds more.
pub(super) const HALT_ANSWER_TIMEOUT: Duration = Duration::from_secs(60);

/// An outer bound on one request, so a helper that stops answering cannot
/// hold a caller forever. Every op already carries a tighter bound of its own
/// inside the helper; this one is only for a helper gone wrong.
pub(super) const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);
