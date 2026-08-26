use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use tokio::sync::{Mutex, Semaphore};
use tokio_util::sync::CancellationToken;

use super::mcp_client::PluginMcpClient;
use super::types::{PluginInvokeError, PluginInvokeResult, RuntimeKey, RuntimeLaunchSpec};
use rmcp::model::ReadResourceResult;

const MAX_GLOBAL_CALLS: usize = 32;
const MAX_INSTANCE_CALLS: usize = 4;
const MAX_RUNTIME_INSTANCES: usize = 16;
const CALL_TIMEOUT: Duration = Duration::from_secs(120);
const DRAIN_TIMEOUT: Duration = Duration::from_secs(1);
const QUARANTINE_THRESHOLD: u32 = 3;

#[derive(Clone)]
pub struct PluginRuntimeSupervisor {
    inner: Arc<SupervisorInner>,
}

pub(super) struct SupervisorInner {
    pub(super) slots: Mutex<BTreeMap<RuntimeKey, Arc<Mutex<SlotState>>>>,
    pub(super) global_calls: Arc<Semaphore>,
    pub(super) shutdown: CancellationToken,
    pub(super) quiescing: AtomicBool,
    pub(super) shutdown_lock: Mutex<()>,
}

pub(super) struct SlotState {
    pub(super) instance: Option<Arc<RuntimeInstance>>,
    consecutive_failures: u32,
    quarantined: bool,
}

pub(super) struct RuntimeInstance {
    pub(super) client: Arc<PluginMcpClient>,
    pub(super) calls: Arc<Semaphore>,
    pub(super) active_leases: AtomicUsize,
    pub(super) last_used: StdMutex<Instant>,
}

impl PluginRuntimeSupervisor {
    pub fn new() -> Self {
        let inner = Arc::new(SupervisorInner {
            slots: Mutex::new(BTreeMap::new()),
            global_calls: Arc::new(Semaphore::new(MAX_GLOBAL_CALLS)),
            shutdown: CancellationToken::new(),
            quiescing: AtomicBool::new(false),
            shutdown_lock: Mutex::new(()),
        });
        super::reaper::spawn(&inner);
        Self { inner }
    }

    pub async fn invoke(
        &self,
        spec: RuntimeLaunchSpec,
        tool_name: String,
        arguments: serde_json::Map<String, serde_json::Value>,
        cancellation: CancellationToken,
        authority_cancellation: CancellationToken,
    ) -> PluginInvokeResult {
        self.ensure_accepting(&cancellation, &authority_cancellation)?;
        let instance = self.ensure_instance(&spec).await?;
        let permits = super::lease::acquire(&self.inner.global_calls, &instance).await?;
        self.ensure_accepting(&cancellation, &authority_cancellation)?;
        let call = instance.client.call_tool(tool_name, arguments);
        let result = tokio::select! {
            _ = cancellation.cancelled() => Err(PluginInvokeError::after_dispatch(
                "plugin_call_cancelled", "Plugin call was cancelled after dispatch",
            )),
            _ = authority_cancellation.cancelled() => Err(PluginInvokeError::after_dispatch(
                "plugin_authority_revoked", "Plugin authority was revoked after dispatch",
            )),
            result = tokio::time::timeout(CALL_TIMEOUT, call) => match result {
                Ok(result) => result,
                Err(_) => Err(PluginInvokeError::after_dispatch(
                    "plugin_call_timeout", "Plugin call timed out after dispatch",
                )),
            }
        };
        drop(permits);
        self.record_outcome(&spec.key, &result).await;
        result
    }

    pub async fn retry_quarantined(&self, key: &RuntimeKey) -> bool {
        let slot = self.inner.slots.lock().await.get(key).cloned();
        let Some(slot) = slot else {
            return false;
        };
        let mut state = slot.lock().await;
        state.quarantined = false;
        state.consecutive_failures = 0;
        true
    }

    pub async fn read_resource(
        &self,
        spec: RuntimeLaunchSpec,
        uri: String,
        cancellation: CancellationToken,
        authority_cancellation: CancellationToken,
    ) -> Result<ReadResourceResult, PluginInvokeError> {
        self.ensure_accepting(&cancellation, &authority_cancellation)?;
        let instance = self.ensure_instance(&spec).await?;
        let permits = super::lease::acquire(&self.inner.global_calls, &instance).await?;
        self.ensure_accepting(&cancellation, &authority_cancellation)?;
        let result = tokio::select! {
            _ = cancellation.cancelled() => Err(PluginInvokeError::after_dispatch(
                "plugin_resource_cancelled", "Plugin resource read was cancelled",
            )),
            _ = authority_cancellation.cancelled() => Err(PluginInvokeError::after_dispatch(
                "plugin_authority_revoked", "Plugin authority was revoked",
            )),
            result = tokio::time::timeout(CALL_TIMEOUT, instance.client.read_resource(uri)) =>
                result.map_err(|_| PluginInvokeError::after_dispatch(
                    "plugin_resource_timeout", "Plugin resource read timed out",
                ))?,
        };
        drop(permits);
        result
    }

    pub async fn shutdown(&self) {
        self.inner.quiescing.store(true, Ordering::Release);
        self.inner.shutdown.cancel();
        let _guard = self.inner.shutdown_lock.lock().await;
        let slots = self
            .inner
            .slots
            .lock()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let instances = collect_instances(&slots).await;
        let deadline = Instant::now() + DRAIN_TIMEOUT;
        while instances
            .iter()
            .any(|instance| instance.active_leases.load(Ordering::Acquire) > 0)
            && Instant::now() < deadline
        {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        for instance in instances {
            instance.client.shutdown().await;
        }
        self.inner.slots.lock().await.clear();
    }

    pub async fn stop_plugin(&self, plugin_slug: &str) {
        let slots = {
            let mut values = self.inner.slots.lock().await;
            let keys = values
                .keys()
                .filter(|key| key.plugin_slug == plugin_slug)
                .cloned()
                .collect::<Vec<_>>();
            keys.into_iter()
                .filter_map(|key| values.remove(&key))
                .collect::<Vec<_>>()
        };
        let instances = collect_instances(&slots).await;
        let deadline = Instant::now() + DRAIN_TIMEOUT;
        while instances
            .iter()
            .any(|instance| instance.active_leases.load(Ordering::Acquire) > 0)
            && Instant::now() < deadline
        {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        for instance in instances {
            instance.client.shutdown().await;
        }
    }

    async fn ensure_instance(
        &self,
        spec: &RuntimeLaunchSpec,
    ) -> Result<Arc<RuntimeInstance>, PluginInvokeError> {
        let slot = {
            let mut slots = self.inner.slots.lock().await;
            if !slots.contains_key(&spec.key) && slots.len() >= MAX_RUNTIME_INSTANCES {
                return Err(PluginInvokeError::before_effect(
                    "plugin_runtime_capacity",
                    "Plugin runtime instance limit reached",
                ));
            }
            slots
                .entry(spec.key.clone())
                .or_insert_with(|| Arc::new(Mutex::new(SlotState::new())))
                .clone()
        };
        let mut state = slot.lock().await;
        if state.quarantined {
            return Err(PluginInvokeError::before_effect(
                "plugin_runtime_quarantined",
                "Plugin runtime requires explicit retry",
            ));
        }
        if let Some(instance) = &state.instance {
            if !instance.client.is_closed().await {
                return Ok(instance.clone());
            }
            state.instance = None;
        }
        match PluginMcpClient::start(spec).await {
            Ok(client) => {
                let instance = Arc::new(RuntimeInstance::new(client));
                state.instance = Some(instance.clone());
                Ok(instance)
            }
            Err(error) => {
                state.consecutive_failures = state.consecutive_failures.saturating_add(1);
                state.quarantined = state.consecutive_failures >= QUARANTINE_THRESHOLD;
                Err(error)
            }
        }
    }

    fn ensure_accepting(
        &self,
        cancellation: &CancellationToken,
        authority_cancellation: &CancellationToken,
    ) -> Result<(), PluginInvokeError> {
        if self.inner.quiescing.load(Ordering::Acquire)
            || cancellation.is_cancelled()
            || authority_cancellation.is_cancelled()
        {
            return Err(PluginInvokeError::before_effect(
                "plugin_runtime_unavailable",
                "Plugin runtime is not accepting calls",
            ));
        }
        Ok(())
    }

    async fn record_outcome(&self, key: &RuntimeKey, result: &PluginInvokeResult) {
        let slot = self.inner.slots.lock().await.get(key).cloned();
        let Some(slot) = slot else {
            return;
        };
        let failed_transport = result
            .as_ref()
            .err()
            .is_some_and(|error| error.code == "plugin_call_failed");
        let client = {
            let mut state = slot.lock().await;
            if result.is_ok() {
                state.consecutive_failures = 0;
                return;
            }
            if !failed_transport {
                return;
            }
            state.consecutive_failures = state.consecutive_failures.saturating_add(1);
            state.quarantined = state.consecutive_failures >= QUARANTINE_THRESHOLD;
            state
                .instance
                .take()
                .map(|instance| instance.client.clone())
        };
        if let Some(client) = client {
            client.shutdown().await;
        }
    }
}

async fn collect_instances(slots: &[Arc<Mutex<SlotState>>]) -> Vec<Arc<RuntimeInstance>> {
    let mut result = Vec::new();
    for slot in slots {
        if let Some(instance) = slot.lock().await.instance.take() {
            result.push(instance);
        }
    }
    result
}

impl Default for PluginRuntimeSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl SlotState {
    fn new() -> Self {
        Self {
            instance: None,
            consecutive_failures: 0,
            quarantined: false,
        }
    }
}

impl RuntimeInstance {
    fn new(client: Arc<PluginMcpClient>) -> Self {
        Self {
            client,
            calls: Arc::new(Semaphore::new(MAX_INSTANCE_CALLS)),
            active_leases: AtomicUsize::new(0),
            last_used: StdMutex::new(Instant::now()),
        }
    }
}
