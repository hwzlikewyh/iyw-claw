use super::{key, ChannelMessageTarget, ChatChannelManager, TypingController};

impl TypingController {
    pub(crate) async fn ensure_running(
        &self,
        manager: &ChatChannelManager,
        target: ChannelMessageTarget,
        identity: (&str, &str),
    ) {
        let Ok(backend) = manager.backend_for(target.channel_id).await else {
            return;
        };
        if !backend.supports_typing() {
            return;
        }
        let (route, owner) = identity;
        let existing = self
            .leases
            .lock()
            .await
            .get(&key(target.channel_id, route))
            .map(|lease| (lease.owner_connection_id == owner, lease.paused));
        match existing {
            Some((true, false)) => {}
            Some((true, true)) => self.resume(manager, target.channel_id, route, owner).await,
            _ => {
                self.start(manager.clone_ref(), target, route.into(), owner.into())
                    .await
            }
        }
    }
}
