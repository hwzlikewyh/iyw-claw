//! 宿主接受回合时绑定渠道来源；模型文本不能创建或修改身份。
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::types::{ChannelMessageTarget, ChannelType};

pub(crate) const DEFAULT_REPORT_SECONDS: u64 = 120;
pub(crate) const MIN_REPORT_SECONDS: u64 = 30;
pub(crate) const MAX_REPORT_SECONDS: u64 = 86_400;
const MAX_TRACKED_TASKS: usize = 512;
const TERMINAL_RETENTION: Duration = Duration::from_secs(3600);

#[derive(Clone)]
pub(crate) struct Route {
    pub channel_id: i32,
    pub sender_id: String,
    pub target_id: String,
    pub target: ChannelMessageTarget,
    pub generation: u64,
}

#[derive(Clone)]
pub(crate) struct PreparedOrigin {
    pub route: Route,
    pub channel_type: ChannelType,
    pub title: String,
    pub report_seconds: u64,
    pub prior_delegations: HashSet<String>,
    pub accepted_event_seq: u64,
}

#[derive(Clone)]
pub(crate) struct TaskContext {
    pub id: String,
    pub connection_id: String,
    pub turn_generation: i64,
    pub origin: PreparedOrigin,
    pub started: Instant,
    pub interval: Duration,
    pub periodic: bool,
    pub next_due: Instant,
    pub last_sent: Option<Instant>,
    pub summary: Option<String>,
    pub summary_at: Option<Instant>,
    pub summary_sent_at: Option<Instant>,
    pub delivery_error: Option<String>,
    pub tools_active: bool,
    pub delegations: HashSet<String>,
    pub background: super::task_progress::background::BackgroundTasks,
    pub phase: String,
    pub ended: Option<Instant>,
    pub in_flight: bool,
    pub revision: u64,
}

#[derive(Default)]
struct Registry {
    routes: HashMap<String, Route>,
    current: HashMap<String, String>,
    tasks: HashMap<String, TaskContext>,
}

fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(Mutex::default)
}

pub(crate) fn bind_route(connection_id: &str, route: Route) {
    let mut registry = registry().lock().unwrap_or_else(|e| e.into_inner());
    for task in registry.tasks.values_mut().filter(|task| {
        task.connection_id == connection_id
            && task.origin.route.generation != route.generation
            && task.ended.is_none()
    }) {
        task.ended = Some(Instant::now());
        task.tools_active = false;
        task.revision = task.revision.saturating_add(1);
    }
    registry.routes.insert(connection_id.to_string(), route);
}

pub(crate) fn remove_route(connection_id: &str) {
    let mut registry = registry().lock().unwrap_or_else(|e| e.into_inner());
    registry.routes.remove(connection_id);
    registry.current.remove(connection_id);
    for task in registry
        .tasks
        .values_mut()
        .filter(|task| task.connection_id == connection_id)
    {
        task.ended = Some(Instant::now());
        task.revision = task.revision.saturating_add(1);
    }
}

pub(crate) fn route(connection_id: &str) -> Option<Route> {
    registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .routes
        .get(connection_id)
        .cloned()
}

pub(crate) fn accept(
    connection_id: &str,
    generation: i64,
    origin: Option<PreparedOrigin>,
) -> Result<(), &'static str> {
    let mut registry = registry().lock().unwrap_or_else(|e| e.into_inner());
    let Some(origin) = origin else {
        for task in registry
            .tasks
            .values_mut()
            .filter(|task| task.connection_id == connection_id)
        {
            task.tools_active = false;
        }
        registry.current.remove(connection_id);
        return Ok(());
    };
    let matches = registry.routes.get(connection_id).is_some_and(|route| {
        route.generation == origin.route.generation
            && route.target.matches_thread(&origin.route.target)
            && route.sender_id == origin.route.sender_id
    });
    if !matches {
        return Err("CHANNEL_CONTEXT_STALE");
    }
    registry.tasks.retain(|_, task| {
        task.ended
            .is_none_or(|at| at.elapsed() < TERMINAL_RETENTION)
    });
    if registry.tasks.len() >= MAX_TRACKED_TASKS {
        registry.tasks.retain(|_, task| task.ended.is_none());
    }
    if registry.tasks.len() >= MAX_TRACKED_TASKS {
        return Err("CHANNEL_TASK_CAPACITY");
    }
    for task in registry
        .tasks
        .values_mut()
        .filter(|task| task.connection_id == connection_id)
    {
        task.tools_active = false;
    }
    let task = new_task(connection_id, generation, origin);
    registry
        .current
        .insert(connection_id.to_string(), task.id.clone());
    registry.tasks.insert(task.id.clone(), task);
    Ok(())
}

fn new_task(connection_id: &str, generation: i64, origin: PreparedOrigin) -> TaskContext {
    let id = uuid::Uuid::new_v4().to_string();
    let started = Instant::now();
    let seconds = origin.report_seconds;
    TaskContext {
        id: id.clone(),
        connection_id: connection_id.to_string(),
        turn_generation: generation,
        origin,
        started,
        interval: Duration::from_secs(seconds),
        periodic: true,
        next_due: started + Duration::from_secs(seconds),
        last_sent: None,
        summary: None,
        summary_at: None,
        summary_sent_at: None,
        delivery_error: None,
        tools_active: true,
        delegations: HashSet::new(),
        background: Default::default(),
        phase: "running".into(),
        ended: None,
        in_flight: false,
        revision: 0,
    }
}

pub(crate) fn current(connection_id: &str) -> Option<TaskContext> {
    let registry = registry().lock().unwrap_or_else(|e| e.into_inner());
    let id = registry.current.get(connection_id)?;
    let route = registry.routes.get(connection_id)?;
    registry
        .tasks
        .get(id)
        .filter(|task| task.ended.is_none() && task.origin.route.generation == route.generation)
        .cloned()
}

pub(crate) fn validate(connection_id: &str, context_id: &str) -> Result<TaskContext, String> {
    current(connection_id)
        .filter(|task| task.id == context_id && task.tools_active)
        .ok_or_else(|| "CHANNEL_CONTEXT_STALE".to_string())
}

pub(crate) fn capability(connection_id: &str) -> Option<TaskContext> {
    current(connection_id).filter(|task| task.tools_active)
}

pub(crate) fn tracked(id: &str) -> Option<TaskContext> {
    let registry = registry().lock().unwrap_or_else(|e| e.into_inner());
    let task = registry.tasks.get(id).filter(|task| task.ended.is_none())?;
    let route = registry.routes.get(&task.connection_id)?;
    (route.generation == task.origin.route.generation).then(|| task.clone())
}

pub(crate) fn tasks() -> Vec<TaskContext> {
    registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .tasks
        .values()
        .filter(|task| task.ended.is_none())
        .cloned()
        .collect()
}

pub(crate) fn update(id: &str, edit: impl FnOnce(&mut TaskContext)) {
    if let Some(task) = registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .tasks
        .get_mut(id)
    {
        edit(task);
    }
}

pub(crate) fn configure(
    id: &str,
    interval: Option<u64>,
    periodic: Option<bool>,
) -> Result<(), String> {
    tracked(id).ok_or("CHANNEL_CONTEXT_STALE")?;
    if interval.is_some_and(|value| !(MIN_REPORT_SECONDS..=MAX_REPORT_SECONDS).contains(&value)) {
        return Err("REPORT_INTERVAL_OUT_OF_RANGE".into());
    }
    update(id, |task| {
        if let Some(seconds) = interval {
            task.interval = Duration::from_secs(seconds);
        }
        if let Some(enabled) = periodic {
            task.periodic = enabled;
        }
        task.next_due = Instant::now() + task.interval;
        task.revision = task.revision.saturating_add(1);
    });
    Ok(())
}

pub(crate) fn finish_turn(connection_id: &str, event_seq: u64) {
    let Some(task) = current(connection_id) else {
        return;
    };
    if event_seq <= task.origin.accepted_event_seq {
        return;
    }
    update(&task.id, |task| {
        task.tools_active = false;
        task.revision = task.revision.saturating_add(1);
    });
}

pub(crate) fn claim(id: &str, revision: u64) -> bool {
    let mut claimed = false;
    update(id, |task| {
        if task.revision == revision && task.ended.is_none() && !task.in_flight {
            task.in_flight = true;
            claimed = true;
        }
    });
    claimed
}

pub(crate) fn matching(target: &ChannelMessageTarget, sender_id: &str) -> Vec<TaskContext> {
    tasks()
        .into_iter()
        .filter(|task| {
            task.origin.route.target.matches_thread(target)
                && task.origin.route.sender_id == sender_id
        })
        .collect()
}

pub(crate) fn delegation_started(connection_id: &str, event_seq: u64, tool_id: &str) {
    let Some(task) =
        current(connection_id).filter(|task| event_seq > task.origin.accepted_event_seq)
    else {
        return;
    };
    update(&task.id, |task| {
        task.delegations.insert(tool_id.to_string());
    });
}
