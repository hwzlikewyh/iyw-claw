use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use anyhow::{ensure, Result};

use crate::model::EnvironmentAction;

const SCALE: u32 = 10_000;
const PREPARE_SHARE: u32 = 8_000;
const DOWNLOAD_SHARE: u32 = 6_000;
const EXTRACTION_SHARE: u32 = 2_500;
static STATE: OnceLock<Mutex<Progress>> = OnceLock::new();

struct Progress {
    path: Option<PathBuf>,
    phase: &'static str,
    value: u32,
    components: BTreeMap<String, (u64, u32)>,
    active: String,
    verified: usize,
    verify_total: usize,
    transaction: String,
}

pub fn configure(path: Option<String>) -> Result<()> {
    let path = path.map(PathBuf::from);
    ensure!(path.as_ref().is_none_or(|path| path.is_absolute()), "progress path must be absolute");
    let _ = STATE.set(Mutex::new(Progress {
        path,
        phase: "prepare",
        value: 0,
        components: BTreeMap::new(),
        active: String::new(),
        verified: 0,
        verify_total: 0,
        transaction: String::new(),
    }));
    with_state(|_| {});
    Ok(())
}

pub fn plan(actions: &[EnvironmentAction]) {
    with_state(|state| {
        state.components = actions
            .iter()
            .map(|action| {
                (
                    action.component_id.clone(),
                    (action.artifact.size_bytes.max(1), 0),
                )
            })
            .collect();
    });
}

pub fn percent() -> Option<u32> {
    STATE.get()?.lock().ok().map(|state| state.value / (SCALE / 100))
}

pub fn event(component: &str, phase: &str, done: u64, total: u64) {
    with_state(|state| match phase {
        "downloading" => state.component(component, fraction(done, total, DOWNLOAD_SHARE)),
        "downloaded" | "cached" => state.component(component, DOWNLOAD_SHARE),
        "extracting" => {
            state.active = component.into();
            state.component(component, DOWNLOAD_SHARE);
        }
        "reused" | "component-prepared" | "optional-skipped" => state.component(component, SCALE),
        "verified" => {
            state.verified += 1;
            state.value = PREPARE_SHARE
                + fraction(
                    state.verified as u64,
                    state.verify_total as u64,
                    SCALE - PREPARE_SHARE,
                );
        }
        "committed" => {
            state.phase = "committed";
            state.value = SCALE;
        }
        _ => {}
    });
}

pub fn extracted(done: u64, total: u64) {
    with_state(|state| {
        let active = state.active.clone();
        state.component(
            &active,
            DOWNLOAD_SHARE + fraction(done, total, EXTRACTION_SHARE),
        );
    });
}

pub fn prepared(transaction: &str) {
    with_state(|state| {
        state.phase = "prepared";
        state.transaction = transaction.into();
        state.value = PREPARE_SHARE;
    });
}

pub fn committing(count: usize) {
    with_state(|state| {
        state.phase = "commit";
        state.value = PREPARE_SHARE;
        state.verify_total = count;
    });
}

fn fraction(done: u64, total: u64, scale: u32) -> u32 {
    if total == 0 {
        return 0;
    }
    ((done.min(total) as u128 * scale as u128) / total as u128) as u32
}

fn with_state(change: impl FnOnce(&mut Progress)) {
    let Some(lock) = STATE.get() else { return };
    let Ok(mut state) = lock.lock() else { return };
    change(&mut state);
    // 进度写入失败不能破坏已有环境；安装结果由子进程退出码确认。
    let _ = state.persist();
}

impl Progress {
    fn component(&mut self, id: &str, value: u32) {
        if let Some((_, completed)) = self.components.get_mut(id) {
            *completed = (*completed).max(value);
        }
        let total: u128 = self
            .components
            .values()
            .map(|(size, _)| *size as u128)
            .sum();
        if total == 0 {
            return;
        }
        let done: u128 = self
            .components
            .values()
            .map(|(size, value)| *size as u128 * *value as u128)
            .sum();
        let percent = (done * PREPARE_SHARE as u128 / total / SCALE as u128) as u32;
        self.value = self.value.max(percent);
    }

    fn persist(&self) -> std::io::Result<()> {
        let Some(path) = self.path.as_ref() else { return Ok(()) };
        let temporary = path.with_extension("new");
        let text = format!(
            "[progress]\nPhase={}\nValue={}\nTransaction={}\n",
            self.phase, self.value, self.transaction
        );
        std::fs::write(&temporary, text)?;
        std::fs::rename(temporary, path)
    }
}
