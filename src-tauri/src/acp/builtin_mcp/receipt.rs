use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData;
use serde_json::Value;

use crate::acp::delegation::companion::PostRelayAction;

use super::delivery::RelayDelivery;

const MAX_RECEIPTS: usize = 1024;
const MAX_RECEIPTS_PER_PARENT: usize = 16;
const RECEIPT_TTL: Duration = Duration::from_secs(25 * 60 * 60);

#[derive(Clone, Default)]
pub(super) struct DeliveryReceiptRegistry {
    inner: Arc<Mutex<HashMap<String, PendingReceipt>>>,
}

struct PendingReceipt {
    parent_connection_id: String,
    issued_at: Instant,
    relayed: bool,
    committing: bool,
    callback: PostRelayAction,
}

impl DeliveryReceiptRegistry {
    pub(super) fn reserve(
        &self,
        parent_connection_id: &str,
        callback: PostRelayAction,
    ) -> Option<String> {
        let mut entries = self.lock();
        prune_expired(&mut entries);
        let parent_count = entries
            .values()
            .filter(|entry| entry.parent_connection_id == parent_connection_id)
            .count();
        if entries.len() >= MAX_RECEIPTS || parent_count >= MAX_RECEIPTS_PER_PARENT {
            return None;
        }
        let receipt = mint_unique_receipt(&entries);
        entries.insert(
            receipt.clone(),
            PendingReceipt {
                parent_connection_id: parent_connection_id.to_string(),
                issued_at: Instant::now(),
                relayed: false,
                committing: false,
                callback,
            },
        );
        Some(receipt)
    }

    pub(super) async fn acknowledge_and_commit(
        &self,
        parent_connection_id: &str,
        receipt: &str,
    ) -> bool {
        let callback = {
            let mut entries = self.lock();
            prune_expired(&mut entries);
            let entry = entries.get_mut(receipt).filter(|entry| {
                entry.relayed
                    && !entry.committing
                    && entry.parent_connection_id == parent_connection_id
            });
            entry.map(|entry| {
                entry.committing = true;
                entry.callback.clone()
            })
        };
        let Some(callback) = callback else {
            return false;
        };
        let committed = callback.run().await;
        let mut entries = self.lock();
        if committed {
            entries.remove(receipt);
            return true;
        }
        if let Some(entry) = entries
            .get_mut(receipt)
            .filter(|entry| entry.parent_connection_id == parent_connection_id)
        {
            entry.committing = false;
        }
        false
    }

    pub(super) async fn acknowledge_required(
        &self,
        parent_connection_id: &str,
        receipt: &str,
    ) -> Result<(), ErrorData> {
        if self
            .acknowledge_and_commit(parent_connection_id, receipt)
            .await
        {
            return Ok(());
        }
        Err(ErrorData::invalid_params(
            "delivery receipt is unavailable",
            None,
        ))
    }

    pub(super) fn attach(
        &self,
        result: &mut CallToolResult,
        delivery: Option<RelayDelivery>,
        parent_connection_id: &str,
        callback: PostRelayAction,
    ) -> bool {
        let Some(delivery) = delivery else {
            return false;
        };
        let Some(receipt) = self.reserve(parent_connection_id, callback) else {
            return false;
        };
        let completed_registry = self.clone();
        let completed_receipt = receipt.clone();
        let aborted_registry = self.clone();
        let aborted_receipt = receipt.clone();
        delivery.register(
            Box::new(move || completed_registry.mark_relayed(&completed_receipt)),
            Box::new(move || aborted_registry.discard(&aborted_receipt)),
        );
        append_receipt(result, &receipt);
        true
    }

    pub(super) fn discard(&self, receipt: &str) {
        self.lock().remove(receipt);
    }

    pub(super) fn remove_parent(&self, parent_connection_id: &str) {
        self.lock()
            .retain(|_, entry| entry.parent_connection_id != parent_connection_id);
    }

    pub(super) fn parent_connection_ids(&self) -> Vec<String> {
        self.lock()
            .values()
            .map(|entry| entry.parent_connection_id.clone())
            .collect()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    fn mark_relayed(&self, receipt: &str) {
        let mut entries = self.lock();
        prune_expired(&mut entries);
        if let Some(entry) = entries.get_mut(receipt) {
            entry.relayed = true;
        }
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, PendingReceipt>> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn prune_expired(entries: &mut HashMap<String, PendingReceipt>) {
    entries.retain(|_, entry| entry.issued_at.elapsed() < RECEIPT_TTL);
}

fn mint_unique_receipt(entries: &HashMap<String, PendingReceipt>) -> String {
    loop {
        let receipt = uuid::Uuid::new_v4().to_string();
        if !entries.contains_key(&receipt) {
            return receipt;
        }
    }
}

fn append_receipt(result: &mut CallToolResult, receipt: &str) {
    if let Some(Value::Object(structured)) = result.structured_content.as_mut() {
        structured.insert("iyw_delivery_receipt".into(), Value::String(receipt.into()));
    }
    result.content.push(Content::text(format!(
        "IYW delivery receipt: {receipt}. Pass it as delivery_ack in the next invoke_iyw_capability call."
    )));
}
