use std::sync::{Arc, OnceLock, RwLock};

use serde::Serialize;

use crate::web::event_bridge::{emit_event, EventEmitter};

pub const SYSTEM_SKILLS_UPDATE_EVENT: &str = "system_skills_update_state";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemSkillsUpdateLifecycle {
    Idle,
    Checking,
    UpdateAvailable,
    Downloading,
    Validating,
    Applying,
    UpToDate,
    BlockedDirty,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemSkillsUpdateState {
    pub seq: u64,
    pub status: SystemSkillsUpdateLifecycle,
    pub current_version: Option<String>,
    pub current_commit: Option<String>,
    pub previous_version: Option<String>,
    pub latest_version: Option<String>,
    pub auto_update: bool,
    pub last_checked_at: Option<String>,
    pub dirty: bool,
    pub error: Option<String>,
}

impl Default for SystemSkillsUpdateState {
    fn default() -> Self {
        Self {
            seq: 0,
            status: SystemSkillsUpdateLifecycle::Idle,
            current_version: None,
            current_commit: None,
            previous_version: None,
            latest_version: None,
            auto_update: true,
            last_checked_at: None,
            dirty: false,
            error: None,
        }
    }
}

type StateHandle = Arc<RwLock<SystemSkillsUpdateState>>;

fn handle() -> &'static StateHandle {
    static HANDLE: OnceLock<StateHandle> = OnceLock::new();
    HANDLE.get_or_init(|| Arc::new(RwLock::new(SystemSkillsUpdateState::default())))
}

pub fn snapshot() -> SystemSkillsUpdateState {
    handle()
        .read()
        .map(|state| state.clone())
        .unwrap_or_else(|poisoned| poisoned.into_inner().clone())
}

pub(super) fn mutate(
    emitter: &EventEmitter,
    update: impl FnOnce(&mut SystemSkillsUpdateState),
) -> SystemSkillsUpdateState {
    let snapshot = {
        let mut state = handle()
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.seq += 1;
        update(&mut state);
        state.clone()
    };
    emit_event(emitter, SYSTEM_SKILLS_UPDATE_EVENT, &snapshot);
    snapshot
}
