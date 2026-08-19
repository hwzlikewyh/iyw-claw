use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use futures::future::join_all;
use tokio::sync::Mutex;
use tokio::task::{AbortHandle, JoinError, JoinHandle};

const DRIVER_PENDING: u8 = 0;
const DRIVER_COMPLETED: u8 = 1;
const DRIVER_FAILED: u8 = 2;

#[derive(Clone, Copy)]
pub(crate) enum RuntimeHostDriverOutcome {
    Clean,
    Failed,
}

impl RuntimeHostDriverOutcome {
    pub(crate) fn from_clean(clean: bool) -> Self {
        if clean {
            Self::Clean
        } else {
            Self::Failed
        }
    }

    fn is_clean(self) -> bool {
        matches!(self, Self::Clean)
    }
}

#[derive(Default)]
pub(crate) struct HostStartups {
    entries: StdMutex<Vec<Arc<HostStartupEntry>>>,
    unclean: Arc<AtomicBool>,
}

pub(crate) struct HostStartupEntry {
    abort: AbortHandle,
    driver: Mutex<Option<JoinHandle<RuntimeHostDriverOutcome>>>,
    state: AtomicU8,
    published: AtomicBool,
    unclean: Arc<AtomicBool>,
}

pub(crate) struct HostDriverShutdownReport {
    pub(crate) reaped: bool,
    pub(crate) clean: bool,
    pub(crate) timed_out: bool,
}

pub(crate) struct HostStartupShutdownReport {
    pub(crate) tracked: usize,
    pub(crate) reaped: bool,
    pub(crate) clean: bool,
}

impl HostStartupEntry {
    fn new(driver: JoinHandle<RuntimeHostDriverOutcome>, unclean: Arc<AtomicBool>) -> Arc<Self> {
        Arc::new(Self {
            abort: driver.abort_handle(),
            driver: Mutex::new(Some(driver)),
            state: AtomicU8::new(DRIVER_PENDING),
            published: AtomicBool::new(false),
            unclean,
        })
    }

    pub(crate) fn mark_published(&self) {
        self.published.store(true, Ordering::Release);
    }

    fn abort(&self) {
        self.abort.abort();
    }

    pub(crate) async fn abort_and_reap(&self) -> HostDriverShutdownReport {
        self.abort();
        self.reap(None).await
    }

    pub(crate) async fn shutdown_and_reap(&self, timeout: Duration) -> HostDriverShutdownReport {
        self.reap(Some(timeout)).await
    }

    pub(crate) fn abort_and_reap_in_background(self: &Arc<Self>) {
        self.abort();
        let entry = Arc::clone(self);
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        runtime.spawn(async move {
            let _ = entry.abort_and_reap().await;
        });
    }

    pub(crate) fn shutdown_in_background(self: &Arc<Self>, timeout: Duration) {
        if self.state.load(Ordering::Acquire) != DRIVER_PENDING {
            return;
        }
        let entry = Arc::clone(self);
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            self.abort();
            return;
        };
        runtime.spawn(async move {
            let _ = entry.shutdown_and_reap(timeout).await;
        });
    }

    async fn reap(&self, timeout: Option<Duration>) -> HostDriverShutdownReport {
        let mut driver = self.driver.lock().await;
        let Some(task) = driver.as_mut() else {
            return self.stored_report();
        };
        // Leave the handle in the slot until join completes so a cancelled
        // reaper releases only the mutex and a later owner can retry.
        let (result, timed_out) = wait_for_driver(task, timeout).await;
        *driver = None;
        let clean = self.record_result(result, timed_out);
        HostDriverShutdownReport {
            reaped: true,
            clean,
            timed_out,
        }
    }

    fn stored_report(&self) -> HostDriverShutdownReport {
        HostDriverShutdownReport {
            reaped: self.state.load(Ordering::Acquire) != DRIVER_PENDING,
            clean: self.state.load(Ordering::Acquire) == DRIVER_COMPLETED,
            timed_out: false,
        }
    }

    fn record_result(
        &self,
        result: Result<RuntimeHostDriverOutcome, JoinError>,
        timed_out: bool,
    ) -> bool {
        let clean = !timed_out
            && match result {
                Ok(outcome) => outcome.is_clean(),
                Err(error) => {
                    tracing::error!(error = %error, "[ACP][host] runtime driver failed while reaping");
                    false
                }
            };
        if !clean && !self.published.load(Ordering::Acquire) {
            self.unclean.store(true, Ordering::Release);
        }
        self.state.store(
            if clean {
                DRIVER_COMPLETED
            } else {
                DRIVER_FAILED
            },
            Ordering::Release,
        );
        clean
    }

    fn requires_tracking(&self) -> bool {
        !self.published.load(Ordering::Acquire)
            && self.state.load(Ordering::Acquire) == DRIVER_PENDING
    }
}

async fn wait_for_driver(
    task: &mut JoinHandle<RuntimeHostDriverOutcome>,
    timeout: Option<Duration>,
) -> (Result<RuntimeHostDriverOutcome, JoinError>, bool) {
    let Some(timeout) = timeout else {
        return ((&mut *task).await, false);
    };
    tokio::select! {
        result = &mut *task => (result, false),
        _ = tokio::time::sleep(timeout) => {
            task.abort();
            ((&mut *task).await, true)
        }
    }
}

impl HostStartups {
    pub(crate) fn register(
        &self,
        driver: JoinHandle<RuntimeHostDriverOutcome>,
    ) -> Arc<HostStartupEntry> {
        let entry = HostStartupEntry::new(driver, Arc::clone(&self.unclean));
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        entries.retain(|current| current.requires_tracking());
        entries.push(Arc::clone(&entry));
        entry
    }

    pub(crate) async fn reap_all(&self) -> HostStartupShutdownReport {
        let entries = {
            let mut registered = self
                .entries
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            registered.retain(|entry| entry.requires_tracking());
            registered.clone()
        };
        let tracked = entries.len();
        let results = join_all(entries.iter().map(|entry| entry.abort_and_reap())).await;
        let mut registered = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        registered.retain(|entry| entry.requires_tracking());
        let background_unclean = self.unclean.swap(false, Ordering::AcqRel);
        HostStartupShutdownReport {
            tracked,
            reaped: results.iter().all(|report| report.reaped) && registered.is_empty(),
            clean: results.iter().all(|report| report.clean) && !background_unclean,
        }
    }
}
