use tokio::sync::watch;

use super::UserMemoryService;

#[derive(Debug)]
pub struct MemoryForegroundGuard {
    active: watch::Sender<usize>,
}

impl Drop for MemoryForegroundGuard {
    fn drop(&mut self) {
        self.active
            .send_modify(|count| *count = count.saturating_sub(1));
    }
}

impl UserMemoryService {
    pub(crate) fn begin_foreground(&self) -> MemoryForegroundGuard {
        self.foreground.send_modify(|count| *count += 1);
        MemoryForegroundGuard {
            active: self.foreground.clone(),
        }
    }

    pub(super) async fn wait_for_foreground(&self) {
        let mut activity = self.foreground.subscribe();
        let _ = activity.wait_for(|count| *count == 0).await;
    }

    pub(super) fn foreground_active(&self) -> bool {
        *self.foreground.borrow() != 0
    }
}
