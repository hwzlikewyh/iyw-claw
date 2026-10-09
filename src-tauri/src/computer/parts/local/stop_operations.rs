use super::*;

impl LocalBackend {
    /// Count the person's `stop`-th Stop here at once — before its `Halt`
    /// has gone anywhere — so no action let through before it leaves this
    /// backend from now on. Told again by [`halt`](ComputerBackend::halt),
    /// which changes nothing then: an older count never replaces a newer.
    pub fn note_stop(&self, stop: u64) {
        self.stopped.fetch_max(stop, Ordering::AcqRel);
    }

    /// Whether a helper may run now: computer use is open here and switched
    /// on in the settings.
    #[cfg(target_os = "macos")]
    pub(in crate::computer::local) async fn switched_on(&self) -> bool {
        let open = self.slot.lock().await.open;
        open && match &self.switch {
            Some(config) => config.is_enabled().await,
            None => true,
        }
    }

    /// Send `op` to the helper if one is running; with none, there is no one
    /// to tell (what a helper started later is sent is held to the Stops
    /// counted here).
    pub(in crate::computer::local) async fn to_running(
        &self,
        op: HelperOp,
        stop: u64,
    ) -> Result<(), BackendError> {
        let connection = self.slot.lock().await.connection.clone();
        match connection.filter(|c| !c.is_closed()) {
            Some(connection) => connection
                .request(op, stop)
                .await?
                .decode::<()>()
                .map_err(BackendError::from),
            None => Ok(()),
        }
    }
}
