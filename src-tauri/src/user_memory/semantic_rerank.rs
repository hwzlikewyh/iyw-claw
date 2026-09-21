use std::time::Duration;

use super::{UserMemoryRecallItem, UserMemoryService};
use crate::app_error::AppCommandError;

const RERANK_TIMEOUT: Duration = Duration::from_millis(700);
const MAX_CANDIDATES: usize = 20;

impl UserMemoryService {
    pub(super) async fn rerank_memory_items(
        &self,
        query: (&str, &super::UserMemoryRecallScope),
        items: &mut Vec<UserMemoryRecallItem>,
    ) {
        if items.len() < 2 {
            return;
        }
        let candidate_count = items.len().min(MAX_CANDIDATES);
        let mut candidates = items[..candidate_count].to_vec();
        let outcome = tokio::time::timeout(RERANK_TIMEOUT, async {
            let config = self.cloud_retrieval_config().await?;
            if !config.rerank_enabled || config.rerank_model.is_empty() {
                return Ok(None);
            }
            let current = self.read_index_source().await?;
            super::semantic_recall::retain_current(&mut candidates, &current, query.1);
            if candidates.len() < 2 {
                return Ok(None);
            }
            let gateway = self.cloud_gateway().await?;
            let documents = candidates
                .iter()
                .map(|item| item.content.clone())
                .collect::<Vec<_>>();
            let order = gateway
                .rerank(&config.rerank_model, query.0, &documents)
                .await?;
            Ok::<_, AppCommandError>(Some(order))
        })
        .await;
        let Ok(Ok(Some(order))) = outcome else {
            return;
        };
        let mut ranked = order
            .into_iter()
            .map(|index| {
                let mut item = candidates[index].clone();
                item.lanes.push("rerank".into());
                item
            })
            .collect::<Vec<_>>();
        ranked.extend_from_slice(&items[candidate_count..]);
        *items = ranked;
    }
}
