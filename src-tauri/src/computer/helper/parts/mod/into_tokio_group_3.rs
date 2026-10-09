// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl RawChannel {
    /// Needs a runtime: tokio's socket wrapper registers with its reactor.
    pub(in crate::computer::helper) fn into_tokio(self) -> std::io::Result<Halves> {
        match self {
            #[cfg(target_os = "macos")]
            RawChannel::Socket(socket) => {
                socket.set_nonblocking(true)?;
                let (r, w) = tokio::net::UnixStream::from_std(socket)?.into_split();
                Ok((Box::new(r), Box::new(w)))
            }
            RawChannel::Stdio => Ok((Box::new(tokio::io::stdin()), Box::new(tokio::io::stdout()))),
        }
    }
}
