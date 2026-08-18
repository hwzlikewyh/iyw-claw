use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};

use sacp::schema::SessionId;

use crate::acp::connection::PendingPermissions;
use crate::acp::file_system_runtime::FileSystemRuntime;
use crate::acp::session_state::SessionState;
use crate::acp::terminal_runtime::TerminalRuntime;
use crate::web::event_bridge::EventEmitter;

pub(crate) struct RuntimeSessionRoute {
    pub(crate) state: Arc<tokio::sync::RwLock<SessionState>>,
    pub(crate) emitter: EventEmitter,
    pub(crate) permissions: PendingPermissions,
    pub(crate) cwd: String,
    pub(crate) file_system: Arc<FileSystemRuntime>,
    pub(crate) terminal: Arc<TerminalRuntime>,
}

#[derive(Clone, Default)]
pub(super) struct SessionRequestRouter {
    routes: Arc<Mutex<RouteTables>>,
}

#[derive(Default)]
struct RouteTables {
    connections: HashMap<String, RouteEntry>,
    sessions: HashMap<String, SessionRouteEntry>,
}

struct RouteEntry {
    generation: u64,
    route: Arc<RuntimeSessionRoute>,
}

struct SessionRouteEntry {
    connection_id: String,
    generation: u64,
    route: Arc<RuntimeSessionRoute>,
}

#[derive(Clone)]
pub(crate) struct RuntimeHostRouteBinding {
    connection_id: String,
    generation: u64,
    router: SessionRequestRouter,
}

pub(crate) struct RuntimeHostRouteLease {
    binding: RuntimeHostRouteBinding,
    on_drop: Option<Box<dyn FnOnce() + Send + 'static>>,
}

impl Drop for RuntimeHostRouteLease {
    fn drop(&mut self) {
        let mut routes = self
            .binding
            .router
            .routes
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if routes
            .connections
            .get(&self.binding.connection_id)
            .is_some_and(|entry| entry.generation == self.binding.generation)
        {
            routes.connections.remove(&self.binding.connection_id);
            routes.sessions.retain(|_, entry| {
                entry.connection_id != self.binding.connection_id
                    || entry.generation != self.binding.generation
            });
        }
        drop(routes);
        if let Some(on_drop) = self.on_drop.take() {
            on_drop();
        }
    }
}

impl RuntimeHostRouteLease {
    pub(crate) fn binding(&self) -> RuntimeHostRouteBinding {
        self.binding.clone()
    }

    pub(crate) fn with_on_drop(mut self, on_drop: impl FnOnce() + Send + 'static) -> Self {
        self.on_drop = Some(Box::new(on_drop));
        self
    }

    pub(crate) fn session_ids(&self) -> Vec<String> {
        self.binding.session_ids()
    }
}

impl RuntimeHostRouteBinding {
    pub(crate) fn bind_session(&self, session_id: String) -> bool {
        let mut routes = self
            .router
            .routes
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(route) = routes
            .connections
            .get(&self.connection_id)
            .filter(|entry| entry.generation == self.generation)
            .map(|entry| Arc::clone(&entry.route))
        else {
            return false;
        };
        routes.sessions.retain(|_, entry| {
            entry.connection_id != self.connection_id || entry.generation != self.generation
        });
        routes.sessions.insert(
            session_id,
            SessionRouteEntry {
                connection_id: self.connection_id.clone(),
                generation: self.generation,
                route,
            },
        );
        true
    }

    fn session_ids(&self) -> Vec<String> {
        let routes = self
            .router
            .routes
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        routes
            .sessions
            .iter()
            .filter_map(|(session_id, entry)| {
                (entry.connection_id == self.connection_id && entry.generation == self.generation)
                    .then(|| session_id.clone())
            })
            .collect()
    }
}

impl SessionRequestRouter {
    pub(super) fn register(
        &self,
        connection_id: String,
        session_id: Option<String>,
        route: RuntimeSessionRoute,
    ) -> RuntimeHostRouteLease {
        let generation = next_route_generation();
        let route = Arc::new(route);
        let mut routes = self
            .routes
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        routes.connections.insert(
            connection_id.clone(),
            RouteEntry {
                generation,
                route: Arc::clone(&route),
            },
        );
        if let Some(session_id) = session_id {
            routes.sessions.insert(
                session_id,
                SessionRouteEntry {
                    connection_id: connection_id.clone(),
                    generation,
                    route,
                },
            );
        }
        RuntimeHostRouteLease {
            binding: RuntimeHostRouteBinding {
                connection_id,
                generation,
                router: self.clone(),
            },
            on_drop: None,
        }
    }

    pub(super) fn resolve(&self, session_id: &SessionId) -> Option<Arc<RuntimeSessionRoute>> {
        self.routes
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .sessions
            .get(session_id.0.as_ref())
            .map(|entry| Arc::clone(&entry.route))
    }
}

fn next_route_generation() -> u64 {
    static GENERATION: AtomicU64 = AtomicU64::new(1);
    GENERATION.fetch_add(1, Ordering::Relaxed)
}
