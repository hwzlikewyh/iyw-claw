use serde_json::{json, Value};

use super::{UpstreamClient, UpstreamError};
use crate::{Capability, SessionBinding, SessionOwner, TurnBinding};

impl UpstreamClient {
    pub(crate) async fn discover_subagents(
        &self,
        method: &str,
        params: &Value,
    ) -> Result<(), UpstreamError> {
        if !matches!(method, "item/started" | "item/completed") {
            return Ok(());
        }
        let Some(parent) = params.get("threadId").and_then(Value::as_str) else {
            return Ok(());
        };
        if self.binding_for(parent).await.is_none() {
            return Ok(());
        }
        let Some(item) = params.get("item") else {
            return Ok(());
        };
        let children = match item.get("type").and_then(Value::as_str) {
            Some("collabAgentToolCall") if item["tool"] == "spawnAgent" => item
                .get("receiverThreadIds")
                .and_then(Value::as_array)
                .map(|ids| ids.iter().filter_map(Value::as_str).collect::<Vec<_>>())
                .unwrap_or_default(),
            Some("subAgentActivity") => item
                .get("agentThreadId")
                .and_then(Value::as_str)
                .into_iter()
                .collect(),
            _ => Vec::new(),
        };
        for child in children {
            if child != parent {
                self.bind_descendant(child).await?;
            }
        }
        Ok(())
    }

    /// 只接受上游线程元数据中能追溯到现有绑定的子线程。
    pub(crate) async fn bind_descendant(&self, thread_id: &str) -> Result<(), UpstreamError> {
        if self.binding_for(thread_id).await.is_some() {
            return Ok(());
        }
        let mut lineage = Vec::new();
        let mut current = thread_id.to_string();
        while self.binding_for(&current).await.is_none() {
            if lineage
                .iter()
                .any(|(id, _): &(String, String)| id == &current)
            {
                return Err(invalid("subagent ancestry contains a cycle"));
            }
            let response = self
                .send(json!({ "method": "thread/read", "params": {
                "threadId": current, "includeTurns": false,
            } }))
                .await?;
            let thread = response
                .get("thread")
                .ok_or_else(|| invalid("subagent metadata is missing"))?;
            if thread.get("id").and_then(Value::as_str) != Some(current.as_str()) {
                return Err(invalid("subagent metadata has a different thread id"));
            }
            let parent = thread
                .get("parentThreadId")
                .and_then(Value::as_str)
                .filter(|parent| !parent.is_empty())
                .ok_or_else(|| invalid("thread is not an owned subagent"))?;
            lineage.push((current, parent.to_string()));
            current = parent.to_string();
        }
        for (child, parent) in lineage.into_iter().rev() {
            self.bind_child(&child, &parent).await?;
        }
        Ok(())
    }

    async fn bind_child(&self, child: &str, parent: &str) -> Result<(), UpstreamError> {
        let mut harness = self.harness.lock().await;
        let parent_binding = harness
            .binding(parent)
            .ok_or_else(|| invalid("subagent parent expired"))?;
        let capabilities = harness
            .session_capabilities(parent)
            .ok_or_else(|| invalid("subagent capabilities are missing"))?;
        if !capabilities.contains(Capability::Subagents) {
            return Err(invalid("subagents are not enabled"));
        }
        let owner = SessionOwner::new(
            parent_binding.connection_id,
            parent_binding.conversation_id,
            parent_binding.generation,
        )
        .map_err(|error| invalid(&error.to_string()))?;
        let binding = SessionBinding::new(owner, child, self.runtime_fingerprint.clone())
            .map_err(|error| invalid(&error.to_string()))?;
        harness.bind_session(binding, capabilities)?;
        drop(harness);
        self.child_parents
            .lock()
            .await
            .insert(child.to_string(), parent.to_string());
        self.synchronize_child_turn(child).await?;
        Ok(())
    }

    pub(crate) async fn descendant_of(&self, child: &str, root: &str) -> bool {
        let parents = self.child_parents.lock().await;
        let mut current = child;
        while let Some(parent) = parents.get(current) {
            if parent == root {
                return true;
            }
            current = parent;
        }
        false
    }

    pub(crate) async fn observe_child_turn(
        &self,
        thread_id: &str,
        turn_id: &str,
    ) -> Result<(), UpstreamError> {
        if !self.child_parents.lock().await.contains_key(thread_id) {
            return Ok(());
        }
        let binding = self
            .binding_for(thread_id)
            .await
            .ok_or_else(|| invalid("subagent binding expired"))?;
        let access = super::thread_lifecycle::session_access(&binding);
        let mut harness = self.harness.lock().await;
        if let Some(active) = harness.active_turn_for(thread_id) {
            if active.turn_id == turn_id {
                return Ok(());
            }
            return Err(invalid("subagent has another active turn"));
        }
        harness.begin_turn(access, TurnBinding::new(thread_id, turn_id)?)?;
        Ok(())
    }

    pub(crate) async fn validate_child_request_turn(
        &self,
        thread_id: &str,
        turn_id: &str,
    ) -> Result<(), UpstreamError> {
        if !self.child_parents.lock().await.contains_key(thread_id) {
            return Ok(());
        }
        if self.active_turn_for(thread_id).await.is_none() {
            self.synchronize_child_turn(thread_id).await?;
        }
        if !self
            .active_turn_for(thread_id)
            .await
            .is_some_and(|turn| turn.turn_id == turn_id && !turn.cancelling)
        {
            return Err(invalid("subagent request is not tied to a running turn"));
        }
        Ok(())
    }

    pub(crate) async fn interrupt_descendants(&self, root: &str) {
        let children: Vec<String> = self.child_parents.lock().await.keys().cloned().collect();
        for child in children {
            if self.descendant_of(&child, root).await
                && self.synchronize_child_turn(&child).await.is_err()
            {
                eprintln!("[星河][worker] unable to refresh subagent state before cancellation");
            }
            if self.descendant_of(&child, root).await
                && self.active_turn_for(&child).await.is_some()
            {
                if self.interrupt_turn_for_thread(&child).await.is_err() {
                    eprintln!("[星河][worker] subagent interrupt was not acknowledged during parent cancellation");
                }
            }
        }
    }

    pub(crate) async fn retire_descendants(&self, root: &str) -> Result<(), UpstreamError> {
        self.interrupt_descendants(root).await;
        let children: Vec<String> = self.child_parents.lock().await.keys().cloned().collect();
        let mut retired = Vec::new();
        for child in children {
            if self.descendant_of(&child, root).await {
                if let Some(binding) = self.binding_for(&child).await {
                    self.harness
                        .lock()
                        .await
                        .retire_session(super::thread_lifecycle::session_access(&binding))?;
                }
                retired.push(child);
            }
        }
        let mut parents = self.child_parents.lock().await;
        for child in retired {
            parents.remove(&child);
        }
        Ok(())
    }
}

fn invalid(message: &str) -> UpstreamError {
    UpstreamError::InvalidRequest(message.into())
}
