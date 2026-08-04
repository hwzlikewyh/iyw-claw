use std::fmt::Display;
use std::time::Instant;

use super::emergency::{replace_stage, restore_stage, write_event};

pub struct StartupStage {
    name: &'static str,
    previous: &'static str,
    started: Instant,
    finished: bool,
}

impl StartupStage {
    pub fn new(name: &'static str) -> Self {
        let previous = replace_stage(name);
        write_event("startup_stage", "begin", name, None);
        tracing::info!(
            target: "iyw_claw_startup",
            stage = name,
            status = "begin",
            "startup stage"
        );
        Self {
            name,
            previous,
            started: Instant::now(),
            finished: false,
        }
    }

    pub fn complete(mut self) {
        self.finish("ok", None);
    }

    pub fn fail(mut self, error: impl Display) {
        self.finish("error", Some(error.to_string()));
    }

    fn finish(&mut self, status: &str, detail: Option<String>) {
        if self.finished {
            return;
        }
        let duration_ms = self.started.elapsed().as_millis() as u64;
        write_event(
            "startup_stage",
            status,
            self.name,
            detail.map(|value| format!("duration_ms={duration_ms}; {value}")),
        );
        tracing::info!(
            target: "iyw_claw_startup",
            stage = self.name,
            status,
            duration_ms,
            "startup stage"
        );
        self.finished = true;
        restore_stage(self.previous);
    }
}

impl Drop for StartupStage {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        let duration_ms = self.started.elapsed().as_millis() as u64;
        write_event(
            "startup_stage",
            "interrupted",
            self.name,
            Some(format!("duration_ms={duration_ms}")),
        );
        restore_stage(self.previous);
    }
}

pub fn run_stage<T, E, F>(name: &'static str, operation: F) -> Result<T, E>
where
    E: Display,
    F: FnOnce() -> Result<T, E>,
{
    let stage = StartupStage::new(name);
    match operation() {
        Ok(value) => {
            stage.complete();
            Ok(value)
        }
        Err(error) => {
            stage.fail(&error);
            Err(error)
        }
    }
}
