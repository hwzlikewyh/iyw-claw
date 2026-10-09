// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl Default for ComputerToolsRuntimeConfig {
    fn default() -> Self {
        Self {
            inner: Arc::new(RwLock::new(ComputerToolsConfig::default())),
            changes: Arc::new(tokio::sync::watch::channel(ComputerToolsConfig::default()).0),
            hook: Arc::new(std::sync::RwLock::new(None)),
            served: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }
}

impl ComputerToolsRuntimeConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn snapshot(&self) -> ComputerToolsConfig {
        self.inner.read().await.clone()
    }

    pub async fn set(&self, mut cfg: ComputerToolsConfig) {
        let mut inner = self.inner.write().await;
        cfg.switched_off = inner.switched_off + u64::from(inner.enabled && !cfg.enabled);
        let before = std::mem::replace(&mut *inner, cfg.clone());
        // Under the write lock: no reader sees the new settings before the
        // hook has acted on them.
        if let Some(hook) = self.hook.read().unwrap_or_else(|p| p.into_inner()).as_ref() {
            hook(&before, &cfg);
        }
        // Published under the write lock, so watchers see changes in the order
        // they were made.
        self.changes.send_replace(cfg);
    }

    /// Run `hook` on every change, inside [`set`](Self::set) and before it
    /// returns, with the settings before and after. For what a change takes
    /// away, which must not wait for a watcher to be scheduled — nor be merged
    /// away when a second change follows before it is. It runs under the
    /// settings' write lock: it must not read them back through this handle.
    /// One hook; a second replaces the first.
    pub fn on_change(
        &self,
        hook: impl Fn(&ComputerToolsConfig, &ComputerToolsConfig) + Send + Sync + 'static,
    ) {
        *self.hook.write().unwrap_or_else(|p| p.into_inner()) = Some(Box::new(hook));
    }

    pub async fn is_enabled(&self) -> bool {
        self.inner.read().await.enabled
    }

    /// Note that this process serves computer use — a computer service has
    /// started: always in the desktop app, and in iyw-claw-server where the
    /// person who runs it lets it share the screen it runs on. Until then
    /// the tools are offered to no agent, whatever the switch says.
    pub fn mark_served(&self) {
        self.served
            .store(true, std::sync::atomic::Ordering::Release);
    }

    /// See [`mark_served`](Self::mark_served).
    pub fn is_served(&self) -> bool {
        self.served.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Whether starting applications and moving windows is offered: computer
    /// use on, and the person's own switch for it on.
    pub async fn is_launch_enabled(&self) -> bool {
        let config = self.inner.read().await;
        config.enabled && config.launch_enabled
    }

    /// Whether the clipboard tools are offered: computer use on, and the
    /// person's own switch for them on.
    pub async fn is_clipboard_enabled(&self) -> bool {
        let config = self.inner.read().await;
        config.enabled && config.clipboard_enabled
    }

    /// Every change from here on.
    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<ComputerToolsConfig> {
        self.changes.subscribe()
    }
}
