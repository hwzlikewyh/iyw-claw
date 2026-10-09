// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

pub struct McpClient {
    pub(in crate::computer::helper::mcp) writer:
        tokio::sync::Mutex<Box<dyn AsyncWrite + Send + Unpin>>,
    pub(in crate::computer::helper::mcp) pending: Pending,
    pub(in crate::computer::helper::mcp) next_id: AtomicU64,
    pub(in crate::computer::helper::mcp) closed: Arc<watch::Sender<bool>>,
    /// Held for a whole call, request and reply. See the module note.
    pub(in crate::computer::helper::mcp) turn: tokio::sync::Mutex<()>,
}
