use crate::support::requires_decision_token;
use crate::{LoomHarness, LoomHarnessError};
use anyhow::Result;
use loom_domain::{ControlAction, ControlActionKind};
use loom_store::LoomStoreTx;

impl LoomHarness {
    pub(crate) fn ingest_control_action_tx(
        &self,
        tx: &mut LoomStoreTx<'_>,
        action: ControlAction,
    ) -> Result<()> {
        if requires_decision_token(&action.kind) && action.decision_token.is_none() {
            return Err(LoomHarnessError::MissingDecisionToken.into());
        }
        match action.kind {
            ControlActionKind::ApproveStart => self.approve_start_tx(tx, action),
            ControlActionKind::ModifyCandidate => self.modify_candidate_tx(tx, action),
            ControlActionKind::CancelCandidate => self.cancel_candidate_tx(tx, action),
            ControlActionKind::RequestTaskChange => self.request_task_change_tx(tx, action),
            ControlActionKind::ResumeTask => self.resume_task_tx(tx, action),
            ControlActionKind::KeepCurrentTask
            | ControlActionKind::ReplaceActive
            | ControlActionKind::ApproveRequest
            | ControlActionKind::RejectRequest => Ok(()),
            _ => Ok(()),
        }
    }

    pub fn ingest_control_action(&self, action: ControlAction) -> Result<()> {
        if requires_decision_token(&action.kind) && action.decision_token.is_none() {
            return Err(LoomHarnessError::MissingDecisionToken.into());
        }
        match action.kind {
            ControlActionKind::ApproveStart => self.approve_start(action),
            ControlActionKind::ModifyCandidate => {
                let mut tx = self.store.begin_tx()?;
                self.modify_candidate_tx(&mut tx, action)?;
                tx.commit()
            }
            ControlActionKind::CancelCandidate => {
                let mut tx = self.store.begin_tx()?;
                self.cancel_candidate_tx(&mut tx, action)?;
                tx.commit()
            }
            ControlActionKind::RequestTaskChange => self.request_task_change(action),
            ControlActionKind::KeepCurrentTask
            | ControlActionKind::ReplaceActive
            | ControlActionKind::ApproveRequest
            | ControlActionKind::RejectRequest => Ok(()),
            ControlActionKind::ResumeTask => self.resume_task(action),
            _ => Ok(()),
        }
    }
}
