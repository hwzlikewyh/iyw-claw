// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Build the helper's runtime, open the channel on it (tokio's socket wrapper
/// registers with its reactor) and serve — then leave without waiting on
/// whatever is still running there: iyw-claw is gone, and the process exits
/// when this returns.
pub(super) fn serve_on_own_runtime(
    open: impl FnOnce() -> std::io::Result<Halves>,
    peer: PeerCheck,
    guard: Option<PeerGuard>,
) -> i32 {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            tracing::error!("no runtime: {e}");
            return EXIT_FAILED;
        }
    };
    let code = runtime.block_on(async move {
        let (reader, writer) = match open() {
            Ok(halves) => halves,
            Err(e) => {
                tracing::error!("could not open the channel: {e}");
                return EXIT_FAILED;
            }
        };
        serve(reader, writer, peer, guard).await
    });
    runtime.shutdown_background();
    code
}

pub(super) enum RawChannel {
    /// macOS: the socketpair end iyw-claw handed over as both stdin and stdout.
    #[cfg(target_os = "macos")]
    Socket(std::os::unix::net::UnixStream),
    Stdio,
}

pub(super) type Halves = (
    Box<dyn AsyncRead + Send + Unpin>,
    Box<dyn AsyncWrite + Send + Unpin>,
);
