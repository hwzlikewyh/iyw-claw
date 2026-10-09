// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// What asking for a window back came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Restore {
    /// It was minimized, or its application hidden, and the application took
    /// the request.
    Asked,
    /// It is neither: there was nothing to do.
    AlreadyShown,
    /// Accessibility does not list it: on another desktop, or not a window a
    /// person can bring up.
    Unlisted,
    /// The application would not answer, or refused; the error it gave.
    Failed(i32),
}

/// Put `pid`'s window `window_id` back on the screen — its application shown
/// again if it is hidden, then the window out of the Dock if it is minimized
/// — as clicking it in the Dock would, except that the application is not
/// brought to the front. `ready` is asked on the same thread just before each
/// change is made, once everything read to decide on it has been read; what
/// it refuses is not done, and its error comes back as it is.
pub async fn restore<E: Send + 'static>(
    pid: u32,
    window_id: u64,
    ready: impl Fn() -> Result<(), E> + Send + 'static,
) -> Result<Restore, E> {
    tokio::task::spawn_blocking(move || restore_now(pid, window_id, ready))
        .await
        .unwrap_or(Ok(Restore::Failed(AX_FAILURE)))
}
