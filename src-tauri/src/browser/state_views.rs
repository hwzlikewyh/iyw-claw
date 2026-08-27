use std::time::{Duration, Instant};

use uuid::Uuid;

use super::error::{BrowserError, BrowserErrorContext};
use super::records::{HostRecord, ViewClaimRecord};
use super::state::BrowserState;
use super::state_hosts::{host_gone, view_conflict};
use super::types::{
    BrowserGenerations, BrowserHostKind, BrowserRuntimeStatus, BrowserViewClaimSnapshot,
    BrowserViewStatus,
};

const CLAIM_TIMEOUT: Duration = Duration::from_secs(15);

impl BrowserState {
    pub fn begin_view_claim(
        &mut self,
        tab_id: &str,
        source_host_id: Option<String>,
        target_host_id: String,
        target_index: usize,
    ) -> Result<BrowserViewClaimSnapshot, BrowserError> {
        self.expire_view_claims();
        if self.runtime.status != BrowserRuntimeStatus::Running {
            return Err(view_conflict("The browser runtime is not running"));
        }
        if self.claims.values().any(|claim| claim.tab_id == tab_id) {
            return Err(view_conflict(
                "The browser tab already has a pending view claim",
            ));
        }
        let tab = self.claim_source_tab(tab_id, source_host_id.as_deref())?;
        let target_host = self.claim_target_host(&target_host_id, target_index)?;
        let target_status = status_for_host(target_host.kind);
        let claim = ViewClaimRecord {
            id: Uuid::new_v4().to_string(),
            tab_id: tab_id.to_string(),
            source_host_generation: source_host_id
                .as_ref()
                .and_then(|id| self.hosts.get(id))
                .map(|host| host.generation),
            source_host_id,
            target_host_id,
            target_index,
            source_view_status: tab.view_status,
            target_view_status: target_status,
            runtime_generation: self.runtime.generation,
            tab_generation: tab.tab_generation,
            source_view_generation: tab.view_generation,
            target_view_generation: tab.view_generation.saturating_add(1),
            target_host_generation: target_host.generation,
            first_frame_seq: None,
            expires_at: Instant::now() + CLAIM_TIMEOUT,
        };
        self.tabs
            .get_mut(tab_id)
            .expect("validated tab")
            .view_status = if claim.source_host_id.is_some() {
            BrowserViewStatus::Detaching
        } else {
            BrowserViewStatus::Attaching
        };
        let snapshot = claim_snapshot(&claim);
        self.claims.insert(claim.id.clone(), claim);
        Ok(snapshot)
    }

    fn claim_source_tab(
        &self,
        tab_id: &str,
        source_host_id: Option<&str>,
    ) -> Result<&super::records::TabRecord, BrowserError> {
        let tab = self
            .tabs
            .get(tab_id)
            .ok_or_else(|| BrowserError::tab_not_found(tab_id))?;
        if tab.host_id.as_deref() != source_host_id {
            return Err(view_conflict(
                "The source browser host no longer owns the tab",
            ));
        }
        if let Some(source_id) = source_host_id {
            let source = self.hosts.get(source_id).ok_or_else(host_gone)?;
            if !source.tab_order.iter().any(|id| id == tab_id) {
                return Err(view_conflict("The source browser host lost the tab"));
            }
        }
        Ok(tab)
    }

    fn claim_target_host(
        &self,
        target_host_id: &str,
        target_index: usize,
    ) -> Result<&HostRecord, BrowserError> {
        let target = self.hosts.get(target_host_id).ok_or_else(host_gone)?;
        if target_index > target.tab_order.len() {
            return Err(view_conflict("The browser tab target index is invalid"));
        }
        Ok(target)
    }

    pub fn acknowledge_view_claim(
        &mut self,
        claim_id: &str,
        expected: &BrowserGenerations,
        seq: u64,
    ) -> Result<BrowserViewClaimSnapshot, BrowserError> {
        self.validate_view_claim(claim_id, expected)?;
        if seq == 0 {
            return Err(view_conflict("The browser claim frame is invalid"));
        }
        let claim = self.claims.get_mut(claim_id).expect("validated claim");
        claim.first_frame_seq = Some(seq);
        Ok(claim_snapshot(claim))
    }

    pub fn commit_view_claim(
        &mut self,
        claim_id: &str,
        expected: &BrowserGenerations,
    ) -> Result<(), BrowserError> {
        self.validate_view_claim(claim_id, expected)?;
        let claim = self.claims.get(claim_id).expect("validated claim").clone();
        if claim.first_frame_seq.is_none() {
            return Err(view_conflict(
                "The target browser host has not drawn its first frame",
            ));
        }
        move_claimed_tab(&mut self.hosts, &claim)?;
        let tab = self.tabs.get_mut(&claim.tab_id).expect("validated tab");
        tab.host_id = Some(claim.target_host_id.clone());
        tab.view_generation = claim.target_view_generation;
        tab.view_status = claim.target_view_status;
        self.claims.remove(claim_id);
        Ok(())
    }

    pub fn abort_view_claim(
        &mut self,
        claim_id: &str,
        expected: &BrowserGenerations,
    ) -> Result<(), BrowserError> {
        self.validate_view_claim(claim_id, expected)?;
        self.abort_view_claim_unchecked(claim_id);
        Ok(())
    }

    pub fn claim_snapshot(&self, claim_id: &str) -> Result<BrowserViewClaimSnapshot, BrowserError> {
        self.claims
            .get(claim_id)
            .map(claim_snapshot)
            .ok_or_else(|| view_conflict("The browser view claim no longer exists"))
    }

    pub fn claim_snapshots(&self) -> Vec<BrowserViewClaimSnapshot> {
        self.claims.values().map(claim_snapshot).collect()
    }

    pub fn expire_view_claim(&mut self, claim_id: &str) -> bool {
        let expired = self
            .claims
            .get(claim_id)
            .is_some_and(|claim| claim.expires_at <= Instant::now());
        if expired {
            self.abort_view_claim_unchecked(claim_id);
        }
        expired
    }

    fn expire_view_claims(&mut self) {
        let expired: Vec<String> = self
            .claims
            .values()
            .filter(|claim| claim.expires_at <= Instant::now())
            .map(|claim| claim.id.clone())
            .collect();
        for claim_id in expired {
            self.abort_view_claim_unchecked(&claim_id);
        }
    }

    pub(super) fn abort_view_claim_unchecked(&mut self, claim_id: &str) {
        let Some(claim) = self.claims.remove(claim_id) else {
            return;
        };
        if let Some(tab) = self.tabs.get_mut(&claim.tab_id) {
            if tab.view_generation == claim.source_view_generation {
                tab.view_status = claim.source_view_status;
            }
        }
    }

    fn validate_view_claim(
        &self,
        claim_id: &str,
        expected: &BrowserGenerations,
    ) -> Result<(), BrowserError> {
        let claim = self
            .claims
            .get(claim_id)
            .ok_or_else(|| view_conflict("The browser view claim no longer exists"))?;
        let tab = self
            .tabs
            .get(&claim.tab_id)
            .ok_or_else(|| BrowserError::tab_not_found(&claim.tab_id))?;
        let target_generation = self
            .hosts
            .get(&claim.target_host_id)
            .map(|host| host.generation);
        let source_generation = claim
            .source_host_id
            .as_ref()
            .and_then(|id| self.hosts.get(id))
            .map(|host| host.generation);
        let valid = claim.runtime_generation == expected.runtime_generation
            && claim.tab_generation == expected.tab_generation
            && claim.target_view_generation == expected.view_generation
            && self.runtime.generation == claim.runtime_generation
            && tab.tab_generation == claim.tab_generation
            && tab.view_generation == claim.source_view_generation
            && target_generation == Some(claim.target_host_generation)
            && source_generation == claim.source_host_generation;
        valid.then_some(()).ok_or_else(|| stale_claim(claim))
    }
}

fn move_claimed_tab(
    hosts: &mut std::collections::BTreeMap<String, HostRecord>,
    claim: &ViewClaimRecord,
) -> Result<(), BrowserError> {
    if claim.source_host_id.as_deref() == Some(&claim.target_host_id) {
        let host = hosts.get_mut(&claim.target_host_id).ok_or_else(host_gone)?;
        host.tab_order.retain(|id| id != &claim.tab_id);
        let index = claim.target_index.min(host.tab_order.len());
        host.tab_order.insert(index, claim.tab_id.clone());
        host.active_tab_id = Some(claim.tab_id.clone());
        host.generation = host.generation.saturating_add(1);
        return Ok(());
    }
    if let Some(source_id) = &claim.source_host_id {
        let source = hosts.get_mut(source_id).ok_or_else(host_gone)?;
        source.tab_order.retain(|id| id != &claim.tab_id);
        if source.active_tab_id.as_deref() == Some(&claim.tab_id) {
            source.active_tab_id = source.tab_order.first().cloned();
        }
        source.generation = source.generation.saturating_add(1);
    }
    let target = hosts.get_mut(&claim.target_host_id).ok_or_else(host_gone)?;
    let index = claim.target_index.min(target.tab_order.len());
    target.tab_order.insert(index, claim.tab_id.clone());
    target.active_tab_id = Some(claim.tab_id.clone());
    target.generation = target.generation.saturating_add(1);
    Ok(())
}

fn claim_snapshot(claim: &ViewClaimRecord) -> BrowserViewClaimSnapshot {
    BrowserViewClaimSnapshot {
        claim_id: claim.id.clone(),
        browser_tab_id: claim.tab_id.clone(),
        source_host_id: claim.source_host_id.clone(),
        target_host_id: claim.target_host_id.clone(),
        target_index: claim.target_index,
        target_status: claim.target_view_status,
        generations: BrowserGenerations {
            runtime_generation: claim.runtime_generation,
            tab_generation: claim.tab_generation,
            view_generation: claim.target_view_generation,
            control_epoch: 0,
        },
        first_frame_seq: claim.first_frame_seq,
        expires_in_ms: claim
            .expires_at
            .saturating_duration_since(Instant::now())
            .as_millis() as u64,
    }
}

fn status_for_host(kind: BrowserHostKind) -> BrowserViewStatus {
    match kind {
        BrowserHostKind::Docked => BrowserViewStatus::Docked,
        BrowserHostKind::Detached => BrowserViewStatus::Detached,
    }
}

fn stale_claim(claim: &ViewClaimRecord) -> BrowserError {
    BrowserError::stale_generation(BrowserErrorContext {
        operation_id: Some(claim.id.clone()),
        browser_tab_id: Some(claim.tab_id.clone()),
        runtime_generation: Some(claim.runtime_generation),
        tab_generation: Some(claim.tab_generation),
        view_generation: Some(claim.target_view_generation),
        control_epoch: None,
        ..BrowserErrorContext::default()
    })
}
