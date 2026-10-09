// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Exit codes, for iyw-claw's log: they are all the helper says to a peer it has
/// refused.
pub const EXIT_OK: i32 = 0;

pub const EXIT_FAILED: i32 = 1;

pub const EXIT_PEER_REFUSED: i32 = 2;

pub const EXIT_UNANCHORED: i32 = 3;

/// iyw-claw's designated requirement, compiled into release builds. See the
/// module note.
pub const PEER_REQUIREMENT: Option<&str> = option_env!("IYW_CLAW_COMPUTER_PEER_REQUIREMENT");

/// The helper's whole life: check the peer, serve it, exit when it goes away.
///
/// (Asking macOS for a permission is not part of it: iyw-claw starts a helper
/// of its own for each request — see `REQUEST_PERMISSION_ARG` — and the
/// binary answers that before it gets here, so nothing iyw-claw itself links
/// can raise a request in iyw-claw's name.)
pub fn run() -> i32 {
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_target(false)
        .try_init();

    let channel = match open_channel() {
        Ok(channel) => channel,
        Err((code, why)) => {
            tracing::error!("refusing to serve: {why}");
            return code;
        }
    };

    // Everything the helper writes lives under its own directory; being there
    // keeps the driver (which inherits the working directory) out of wherever
    // iyw-claw happened to be started. Done before any thread exists, and not
    // optional: a helper that stayed where it was started would hand that
    // directory to the driver.
    if let Some(dir) = driver_proc::helper_data_dir() {
        if let Err(e) = std::fs::create_dir_all(&dir).and_then(|_| std::env::set_current_dir(&dir))
        {
            tracing::error!("could not move to {}: {e}", dir.display());
            return EXIT_FAILED;
        }
    }

    serve_on_own_runtime(
        move || channel.raw.into_tokio(),
        channel.peer,
        channel.guard,
    )
}
