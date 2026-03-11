use crate::support::requires_decision_token;
use crate::{LoomHarness, LoomHarnessError};
use anyhow::Result;
use loom_domain::{ControlAction, ControlActionKind};

impl LoomHarness {
    pub fn ingest_control_action(&self, action: ControlAction) -> Result<()> {
        if requires_decision_token(&action.kind) && action.decision_token.is_none() {
            return Err(LoomHarnessError::MissingDecisionToken.into());
        }
        match action.kind {
            ControlActionKind::ApproveStart => self.approve_start(action),
            ControlActionKind::ModifyCandidate => self.modify_candidate(action),
            ControlActionKind::CancelCandidate => self.cancel_candidate(action),
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
