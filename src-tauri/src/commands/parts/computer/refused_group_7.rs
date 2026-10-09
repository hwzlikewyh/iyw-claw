// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl Refusal {
    pub(in crate::commands::computer) fn refused(slug: &'static str, note: String) -> Self {
        Self {
            slug,
            note,
            outcome: ActivityOutcome::Refused,
            maybe_done: false,
        }
    }

    pub(in crate::commands::computer) fn failed(slug: &'static str, note: String) -> Self {
        Self {
            slug,
            note,
            outcome: ActivityOutcome::Failed,
            maybe_done: false,
        }
    }

    pub(in crate::commands::computer) fn maybe_done(self) -> Self {
        Self {
            maybe_done: true,
            ..self
        }
    }
}
