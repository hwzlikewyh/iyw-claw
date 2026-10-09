// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The helper app `helper` is in, or `helper` when it is in none.
pub(super) fn app_of(helper: PathBuf) -> PathBuf {
    helper
        .ancestors()
        .find(|p| p.file_name().is_some_and(|n| n == HELPER_APP))
        .map_or_else(|| helper.clone(), Path::to_path_buf)
}

pub(super) type Pending = Arc<StdMutex<HashMap<u64, oneshot::Sender<HelperReply>>>>;

/// One running, checked helper.
pub(super) struct Connection {
    pub(in crate::computer::local) writer: Mutex<Box<dyn AsyncWrite + Send + Unpin>>,
    pub(in crate::computer::local) pending: Pending,
    pub(in crate::computer::local) next_id: AtomicU64,
    pub(in crate::computer::local) closed: watch::Receiver<bool>,
    /// A write that did not finish: the socket is no longer framed.
    pub(in crate::computer::local) broken: AtomicBool,
    pub(in crate::computer::local) peer: PeerCheck,
    pub(in crate::computer::local) child: HelperChild,
}

pub(super) enum HelperChild {
    #[cfg(target_os = "macos")]
    Mac(crate::computer::spawn::Child),
    #[cfg(not(target_os = "macos"))]
    Tokio(Mutex<tokio::process::Child>),
}
