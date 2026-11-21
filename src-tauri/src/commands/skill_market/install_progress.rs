use std::future::Future;

use sea_orm::DatabaseConnection;

use super::types::SkillInstallPlanItem;
use crate::app_error::AppCommandError;
use crate::web::event_bridge::{emit_event, EventEmitter};

tokio::task_local! {
    static CHANNEL: (EventEmitter, String);
}

pub(crate) async fn with_events<F: Future>(
    task_id: Option<String>,
    emitter: EventEmitter,
    install: F,
) -> F::Output {
    let Some(id) = task_id.and_then(|id| uuid::Uuid::parse_str(&id).ok()) else {
        return install.await;
    };
    let channel = format!("skill-market://install-progress/{id}");
    CHANNEL.scope((emitter, channel), install).await
}

fn report(phase: &str, received: u64, total: Option<u64>) {
    let _ = CHANNEL.try_with(|(emitter, channel)| {
        emit_event(
            emitter,
            channel,
            serde_json::json!({
                "phase": phase, "receivedBytes": received, "totalBytes": total
            }),
        );
    });
}

#[derive(Default)]
pub(super) struct PlanProgress {
    completed_bytes: u64,
    known_total: u64,
    unknown_sizes: usize,
}

impl PlanProgress {
    pub(super) fn new(items: &[SkillInstallPlanItem]) -> Self {
        let mut plan = Self::default();
        for item in items {
            let size = item.download.artifact_size;
            plan.known_total = plan.known_total.saturating_add(size);
            plan.unknown_sizes += usize::from(size == 0);
        }
        plan
    }

    pub(super) async fn download(
        &mut self,
        conn: &DatabaseConnection,
        item: &SkillInstallPlanItem,
    ) -> Result<Vec<u8>, AppCommandError> {
        let mut accounted_size = item.download.artifact_size;
        let bytes = super::download_ticket::download_package(conn, item, |received, size| {
            // 旧包的准确长度来自下载票据，同一包重试时只计入一次。
            if accounted_size != size {
                self.unknown_sizes -= usize::from(accounted_size == 0);
                self.known_total = self
                    .known_total
                    .saturating_sub(accounted_size)
                    .saturating_add(size);
                accounted_size = size;
            }
            report(
                "downloading",
                self.completed_bytes.saturating_add(received),
                self.total(),
            );
        })
        .await?;
        self.completed_bytes = self.completed_bytes.saturating_add(bytes.len() as u64);
        report("verifying", self.completed_bytes, self.total());
        Ok(bytes)
    }

    pub(super) fn installing(&self) {
        report("installing", self.completed_bytes, self.total());
    }

    fn total(&self) -> Option<u64> {
        (self.unknown_sizes == 0).then_some(self.known_total)
    }
}
