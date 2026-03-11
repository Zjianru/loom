mod flows;
mod support;

use anyhow::{Result, bail};
use loom_domain::{
    CurrentTurnEnvelope, HostCapabilitySnapshot, HostSessionId, IngressMeta, InteractionLane,
    SemanticDecisionEnvelope, new_id,
};
use loom_store::LoomStore;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LoomHarnessError {
    #[error("missing decision_token for window-consuming control action")]
    MissingDecisionToken,
    #[error("no open pending decision window matched the provided decision_token")]
    StaleDecisionToken,
    #[error("managed task not found: {0}")]
    ManagedTaskNotFound(String),
    #[error("host capability snapshot missing for session: {0}")]
    MissingCapabilitySnapshot(String),
    #[error("semantic decision is missing managed task class or work horizon")]
    MissingManagedTaskSemantics,
    #[error("managed task active lane requires managed_task_ref")]
    MissingManagedTaskRefForActiveLane,
}

#[derive(Clone)]
pub struct LoomHarness {
    store: LoomStore,
}

impl LoomHarness {
    pub fn new(store: LoomStore) -> Self {
        Self { store }
    }

    pub fn store(&self) -> &LoomStore {
        &self.store
    }

    pub fn ingest_current_turn(&self, turn: CurrentTurnEnvelope) -> Result<()> {
        if !self
            .store
            .record_ingress_receipt(&turn.meta, "current_turn", &turn)?
        {
            return Ok(());
        }
        self.store.save_current_turn(&turn)?;
        Ok(())
    }
}

pub(crate) trait LoomStoreTaskCapabilityExt {
    fn latest_capability_snapshot_for_task(
        &self,
        managed_task_ref: &str,
    ) -> Result<Option<HostCapabilitySnapshot>>;
}

impl LoomStoreTaskCapabilityExt for LoomStore {
    fn latest_capability_snapshot_for_task(
        &self,
        managed_task_ref: &str,
    ) -> Result<Option<HostCapabilitySnapshot>> {
        let task = self
            .load_managed_task(&managed_task_ref.to_string())?
            .ok_or_else(|| LoomHarnessError::ManagedTaskNotFound(managed_task_ref.to_string()))?;
        self.latest_capability_snapshot(&task.host_session_id)
    }
}

pub fn build_current_turn(
    host_session_id: impl Into<HostSessionId>,
    text: impl Into<String>,
) -> CurrentTurnEnvelope {
    CurrentTurnEnvelope {
        meta: IngressMeta::default(),
        host_session_id: host_session_id.into(),
        host_message_ref: Some(new_id("host-message")),
        text: text.into(),
        workspace_ref: Some("/Users/codez/.openclaw".into()),
        repo_ref: Some("openclaw".into()),
    }
}

pub fn ensure_supported_candidate(decision: &SemanticDecisionEnvelope) -> Result<()> {
    if decision.interaction_lane == InteractionLane::ManagedTaskCandidate
        && (decision.managed_task_class.is_none() || decision.work_horizon.is_none())
    {
        bail!(LoomHarnessError::MissingManagedTaskSemantics);
    }
    Ok(())
}
