// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl LocalBackend {
    /// `on_status` is told every status change, for the panel.
    pub fn new(on_status: impl Fn(&BackendStatus) + Send + Sync + 'static) -> Self {
        Self {
            slot: Mutex::new(Slot {
                connection: None,
                open: false,
            }),
            status: StdMutex::new(BackendStatus {
                state: BackendState::Idle,
                detail: None,
                driver_version: driver::DRIVER_VERSION.to_string(),
                peer: None,
            }),
            on_status: Box::new(on_status),
            redownloaded: AtomicBool::new(false),
            stopped: AtomicU64::new(0),
            closing: Mutex::new(()),
            switch: None,
        }
    }

    /// Start no helper unless `config` says computer use is on at that
    /// moment. [`open`](Self::open) is told by a watcher of the settings, which
    /// may be a change behind: one still acting on an earlier "on" must not
    /// start a helper — and fetch a driver just removed — after the switch
    /// has gone off.
    pub fn with_switch(
        mut self,
        config: crate::acp::computer_tools::ComputerToolsRuntimeConfig,
    ) -> Self {
        self.switch = Some(config);
        self
    }

    pub(in crate::computer::local) fn set_status(
        &self,
        state: BackendState,
        detail: Option<String>,
        peer: Option<PeerCheck>,
    ) {
        let snapshot = {
            let mut status = self.status.lock().unwrap_or_else(|p| p.into_inner());
            status.state = state;
            status.detail = detail;
            status.peer = peer;
            status.clone()
        };
        (self.on_status)(&snapshot);
    }

    /// Allow a helper to be started (computer use is on). Starts none.
    pub async fn open(&self) {
        self.slot.lock().await.open = true;
    }

    /// Stop the helper, if one is running, and start none until [`open`] —
    /// computer use was switched off. Taken under the same lock a start is
    /// made under, so once this returns no helper runs and none will. The
    /// helper is let go as when iyw-claw quits, so its driver — even one still
    /// starting — is gone with it.
    ///
    /// [`open`]: Self::open
    pub async fn close(&self) {
        let _closing = self.closing.lock().await;
        self.close_locked().await;
    }

    pub(in crate::computer::local) async fn close_locked(&self) {
        let connection = {
            let mut slot = self.slot.lock().await;
            slot.open = false;
            slot.connection.take()
        };
        if let Some(connection) = connection {
            connection.shut_down().await;
        }
        self.set_status(BackendState::Idle, None, None);
    }

    /// [`close`](Self::close), killing the driver first — whatever it is in the
    /// middle of — rather than letting it finish its call. For removing the
    /// driver: nothing of it may still be running after this returns, which is
    /// also why it waits for a close already under way.
    ///
    /// The helper answers a `Halt` once no driver runs: at once, or — when one
    /// is still starting — once that one has started and been stopped for the
    /// Stop it met, which the helper's own bounds on a start keep under
    /// [`HALT_ANSWER_TIMEOUT`]. Only a helper gone wrong is not waited for
    /// past that.
    pub async fn close_now(&self) {
        let _closing = self.closing.lock().await;
        let connection = self.slot.lock().await.connection.clone();
        if let Some(connection) = connection.filter(|c| !c.is_closed()) {
            // Everything: the helper goes, and nothing it was asked before
            // is served.
            let halted = tokio::time::timeout(
                HALT_ANSWER_TIMEOUT,
                connection.request(HelperOp::Halt { stop: STOP_ALL }, STOP_ALL),
            )
            .await;
            if !matches!(halted, Ok(Ok(_))) {
                tracing::warn!("[computer] the helper did not confirm killing the driver");
            }
        }
        self.close_locked().await;
    }

    /// The running helper, launching (and first fetching the driver for) one
    /// if there is none.
    pub(in crate::computer::local) async fn connection(
        &self,
    ) -> Result<Arc<Connection>, BackendError> {
        let mut slot = self.slot.lock().await;
        let switched_on = match &self.switch {
            Some(config) => config.is_enabled().await,
            None => true,
        };
        if !slot.open || !switched_on {
            return Err(BackendError::Unavailable(
                "computer use is switched off".into(),
            ));
        }
        if let Some(connection) = slot.connection.as_ref().filter(|c| !c.is_closed()) {
            return Ok(connection.clone());
        }
        if let Some(dead) = slot.connection.take() {
            dead.stop().await;
        }
        match self.connect().await {
            Ok(connection) => {
                self.set_status(BackendState::Ready, None, Some(connection.peer));
                slot.connection = Some(connection.clone());
                Ok(connection)
            }
            Err(e) => {
                self.set_status(BackendState::Failed, Some(e.to_string()), None);
                Err(e)
            }
        }
    }

    pub(in crate::computer::local) async fn connect(
        &self,
    ) -> Result<Arc<Connection>, BackendError> {
        // Said only when there is something to fetch: the settings page shows
        // this as an install.
        if driver::cached_driver_path().is_none() {
            self.set_status(BackendState::Downloading, None, None);
        }
        let driver_path = driver::ensure_driver(|_| {})
            .await
            .map_err(|e| BackendError::Unavailable(format!("could not fetch cua-driver: {e}")))?;
        self.set_status(BackendState::Starting, None, None);
        let helper = helper_to_run().await?;
        let connection = launch(&helper).await?;
        let configured = connection
            .request(
                HelperOp::Configure {
                    driver_path: driver_path.to_string_lossy().to_string(),
                    driver_version: driver::DRIVER_VERSION.to_string(),
                },
                self.stopped.load(Ordering::Acquire),
            )
            .await?
            .decode::<()>();
        if let Err(e) = configured {
            connection.stop().await;
            return Err(e.into());
        }
        Ok(connection)
    }

    /// Send one op and decode its answer, restarting the helper once if it
    /// turns out to have died since the last call.
    ///
    /// Never for an action: a helper that died with the request on its way
    /// may have died after delivering it, and a second send would do it
    /// twice. Actions are sent once (see `act`).
    pub(in crate::computer::local) async fn call<T: serde::de::DeserializeOwned>(
        &self,
        op: HelperOp,
    ) -> Result<T, BackendError> {
        // A read is held to the Stops counted as it goes out; iyw-claw withholds
        // one that a later Stop overtakes anyway.
        let stop = self.stopped.load(Ordering::Acquire);
        let connection = self.connection().await?;
        let reply = match connection.request(op.clone(), stop).await {
            Err(BackendError::Unavailable(_)) if connection.is_closed() => {
                // Died between calls. One fresh start, then whatever it says.
                self.connection().await?.request(op, stop).await?
            }
            other => other?,
        };
        self.decode(reply).await
    }

    pub(in crate::computer::local) async fn decode<T: serde::de::DeserializeOwned>(
        &self,
        reply: HelperReply,
    ) -> Result<T, BackendError> {
        match reply.decode::<T>() {
            Err(e) if e.code == HelperErrorCode::DriverRejected => {
                Err(self.driver_rejected(e).await)
            }
            other => other.map_err(BackendError::from),
        }
    }

    /// The cached driver failed the helper's checks: a damaged download, or a
    /// replaced one. Throw it away once so the next call fetches a clean copy;
    /// a second failure is reported as it is.
    pub(in crate::computer::local) async fn driver_rejected(&self, e: HelperError) -> BackendError {
        tracing::error!(
            "[computer] the helper rejected the cached cua-driver: {}",
            e.message
        );
        if !self.redownloaded.swap(true, Ordering::AcqRel) {
            let connection = self.slot.lock().await.connection.take();
            if let Some(connection) = connection {
                connection.stop().await;
            }
            if let Err(clear) = driver::forget_cached_driver().await {
                tracing::warn!("[computer] could not clear the cached cua-driver: {clear}");
            }
        }
        BackendError::Rejected(e.message)
    }
}
