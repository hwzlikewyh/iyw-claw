use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;

use super::types::BrowserAgentIdentity;

mod actions;
#[cfg(all(target_pointer_width = "64", not(target_vendor = "win7")))]
mod engine;
#[cfg(not(all(target_pointer_width = "64", not(target_vendor = "win7"))))]
#[path = "unsupported.rs"]
mod engine;
mod handoff;
mod route;
mod script;
mod tabs;
mod worker;

type TaskKey = (String, Option<i32>, i64);
type Tasks = HashMap<TaskKey, Arc<Task>>;

#[derive(Debug)]
pub(in crate::browser) struct ObscuraRoutes {
    tasks: Mutex<Tasks>,
    slots: Arc<Semaphore>,
}

#[derive(Debug)]
struct Task {
    cancellation: CancellationToken,
    route: Mutex<Route>,
}

#[derive(Debug)]
struct Route {
    slots: Arc<Semaphore>,
    worker: Option<worker::Worker>,
    tabs: BTreeMap<String, Tab>,
    active: Option<String>,
    managed: bool,
}

#[derive(Debug)]
struct Tab {
    _slot: OwnedSemaphorePermit,
    id: String,
    target: String,
    session: String,
    url: String,
    title: String,
    epoch: u64,
    interacted: bool,
}

impl Tab {
    fn new(target: String, session: String, slot: OwnedSemaphorePermit) -> Self {
        Self {
            _slot: slot,
            id: format!("obscura:{}", uuid::Uuid::new_v4().simple()),
            target,
            session,
            url: "about:blank".into(),
            title: String::new(),
            epoch: 0,
            interacted: false,
        }
    }
}

impl ObscuraRoutes {
    async fn existing(&self, identity: &BrowserAgentIdentity) -> Option<Arc<Task>> {
        self.tasks.lock().await.get(&task_key(identity)).cloned()
    }

    async fn task(&self, identity: &BrowserAgentIdentity) -> Option<Arc<Task>> {
        let key = task_key(identity);
        let mut tasks = self.tasks.lock().await;
        if !tasks.contains_key(&key) && tasks.len() >= super::state::MAX_BROWSER_TABS {
            return None;
        }
        Some(
            tasks
                .entry(key)
                .or_insert_with(|| {
                    Arc::new(Task {
                        cancellation: CancellationToken::new(),
                        route: Mutex::new(Route {
                            slots: self.slots.clone(),
                            worker: None,
                            tabs: BTreeMap::new(),
                            active: None,
                            managed: false,
                        }),
                    })
                })
                .clone(),
        )
    }

    pub async fn finish(&self, connection: &str, turn: Option<i64>) {
        self.tasks.lock().await.retain(|key, task| {
            let remove = key.0 == connection && turn.is_none_or(|turn| key.2 == turn);
            if remove {
                task.cancellation.cancel();
            }
            !remove
        });
    }

    pub async fn clear(&self) {
        let mut tasks = self.tasks.lock().await;
        for task in tasks.values() {
            task.cancellation.cancel();
        }
        tasks.clear();
    }
}

impl Default for ObscuraRoutes {
    fn default() -> Self {
        Self {
            tasks: Mutex::default(),
            slots: Arc::new(Semaphore::new(super::state::MAX_BROWSER_TABS)),
        }
    }
}

impl Route {
    fn project(&self, target: Option<&str>, output: Option<Value>) -> Value {
        let status = if self.worker.is_some() {
            "running"
        } else {
            "ready"
        };
        let tabs = self.tabs.values().map(|tab| json!({
            "browserTabId":tab.id, "url":tab.url, "title":tab.title,
            "status":"live", "viewStatus":"unclaimed", "controlStatus":"idle",
            "documentEpoch":tab.epoch, "pageErrorCount":0,
            "generations":{"runtimeGeneration":0,"tabGeneration":tab.epoch,"viewGeneration":0,"controlEpoch":0}
        })).collect::<Vec<_>>();
        json!({"ok":true,"provider":"obscura","runtime":{"status":status,"generation":0,"iywLoginStatus":"unknown"},
            "activeTabId":self.active,"targetTabId":target,"tabs":tabs,"dialogs":[],"downloads":[],"output":output})
    }
}

fn task_key(identity: &BrowserAgentIdentity) -> TaskKey {
    (
        identity.connection_id.clone(),
        identity.conversation_id,
        identity.turn_generation,
    )
}
