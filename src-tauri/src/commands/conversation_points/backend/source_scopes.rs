use super::{Actor, LedgerSource};
use crate::acp::runtime_task_receipts::TaskReceipt;
use std::collections::HashSet;

impl LedgerSource {
    pub(super) fn append(&mut self, actor: &Actor, receipt: TaskReceipt) {
        if actor.own && !receipt.message_id.is_empty() {
            self.messages.insert(
                receipt.message_id.clone(),
                format!("{}:{}", receipt.thread_id, receipt.turn_id),
            );
        }
        if actor.own && receipt.parent_tool_use_id.is_none() {
            self.own.insert(receipt.request_id.clone());
        }
        let mut scopes = actor.scopes.clone();
        scopes.extend(self.subagent_scopes(&receipt));
        let goals = self.receipt_goals(actor, &receipt);
        self.unscoped_descendants |=
            (!receipt.goal_identity_known && goals.is_empty()) || (!actor.own && goals.is_empty());
        scopes.extend(goals.into_iter().map(|goal| format!("goal:{goal}")));
        for scope in scopes {
            self.scopes
                .entry(scope)
                .or_default()
                .insert(receipt.request_id.clone());
        }
        self.requests.push(receipt);
    }

    fn receipt_goals(&self, actor: &Actor, receipt: &TaskReceipt) -> Vec<String> {
        let mut goals: Vec<_> = actor
            .scopes
            .iter()
            .filter_map(|scope| {
                scope
                    .strip_prefix("delegation:")
                    .and_then(|tool| self.tool_goals.get(tool))
                    .cloned()
            })
            .collect();
        if let Some(goal) = receipt
            .root_turn_id
            .as_ref()
            .and_then(|turn| self.goal_turns.get(turn))
        {
            goals.push(goal.clone());
        }
        if let Some(goal) = &receipt.task_id {
            goals.push(goal.clone());
        }
        goals
    }

    fn subagent_scopes(&self, receipt: &TaskReceipt) -> Vec<String> {
        let mut current = receipt.parent_tool_use_id.as_deref();
        let mut scopes = Vec::new();
        let mut seen = HashSet::new();
        while let Some(tool) = current {
            if !seen.insert(tool) {
                break;
            }
            scopes.push(format!("subagent:{tool}"));
            current = self.tool_parents.get(tool).map(String::as_str);
        }
        scopes
    }
}
