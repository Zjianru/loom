use crate::{LoomHarness, LoomHarnessError, LoomStoreTaskCapabilityExt};
use anyhow::{Result, anyhow};
use loom_approval::issue_execution_authorization;
use loom_domain::{
    ApprovalRequestAction, ApprovalRequestPayload, ApprovalScope, KernelOutboundPayload,
    PendingDecisionWindow, PendingDecisionWindowKind, PendingDecisionWindowStatus, ProposedAction,
    RenderEmphasis, RenderHint, RenderTone, RiskAssessment, RiskConsequence, ToolDecisionPayload,
    ToolDecisionValue, new_id, now_timestamp,
};
use loom_risk::assess_action_override;
use serde_json::json;

impl LoomHarness {
    pub fn evaluate_proposed_action(&self, proposal: ProposedAction) -> Result<RiskAssessment> {
        let capability = self
            .store
            .latest_capability_snapshot_for_task(&proposal.managed_task_ref)?
            .ok_or_else(|| {
                LoomHarnessError::MissingCapabilitySnapshot(proposal.managed_task_ref.clone())
            })?;
        let task = self
            .store
            .load_managed_task(&proposal.managed_task_ref)?
            .ok_or_else(|| {
                LoomHarnessError::ManagedTaskNotFound(proposal.managed_task_ref.clone())
            })?;
        let current_scope = self
            .store
            .latest_scope_snapshot(&proposal.managed_task_ref)?
            .ok_or_else(|| anyhow!("scope missing for task {}", proposal.managed_task_ref))?;
        let current_run_ref = task
            .active_run_ref
            .clone()
            .ok_or_else(|| anyhow!("run missing for task {}", proposal.managed_task_ref))?;
        let current_run = self
            .store
            .load_task_run(&current_run_ref)?
            .ok_or_else(|| anyhow!("run payload missing for task {}", proposal.managed_task_ref))?;
        let current_auth =
            task.current_execution_authorization_ref
                .clone()
                .and_then(|authorization_id| {
                    self.store
                        .load_execution_authorization(&authorization_id)
                        .ok()
                        .flatten()
                });

        let override_assessment = assess_action_override(
            &proposal,
            &capability,
            format!("proposed action override: {}", proposal.reason),
        );
        self.store.save_risk_assessment(&override_assessment)?;
        self.log_event(
            &proposal.managed_task_ref,
            "risk_assessment.created",
            json!(override_assessment),
        )?;

        if let Some(current_auth) = current_auth {
            self.store.update_authorization_status(
                &current_auth.authorization_id,
                loom_domain::ExecutionAuthorizationStatus::Superseded,
            )?;
            let narrowed_auth = issue_execution_authorization(
                proposal.managed_task_ref.clone(),
                &current_run,
                &capability,
                &current_scope,
                &override_assessment,
                "action override narrowed authorization",
                Some(current_auth.authorization_id),
                true,
            );
            self.store.save_execution_authorization(&narrowed_auth)?;
            self.log_event(
                &proposal.managed_task_ref,
                "execution_authorization.narrowed",
                json!({
                    "managed_task_ref": proposal.managed_task_ref,
                    "authorization_id": narrowed_auth.authorization_id,
                    "overall_risk_band": override_assessment.overall_risk_band,
                    "scope_version": current_scope.scope_version,
                    "derived_consequences": override_assessment.derived_consequences,
                    "supersedes": narrowed_auth.supersedes,
                    "capability_snapshot_ref": capability.capability_snapshot_ref,
                }),
            )?;
            let mut task = task;
            task.current_execution_authorization_ref = Some(narrowed_auth.authorization_id.clone());
            task.updated_at = now_timestamp();
            self.store.save_managed_task(&task)?;
        }

        if override_assessment
            .derived_consequences
            .iter()
            .any(|consequence| matches!(consequence, RiskConsequence::RequireApprovalWindow))
        {
            let approval_window = PendingDecisionWindow {
                window_id: new_id("window"),
                managed_task_ref: proposal.managed_task_ref.clone(),
                kind: PendingDecisionWindowKind::ApprovalRequest,
                decision_token: new_id("decision"),
                status: PendingDecisionWindowStatus::Open,
                allowed_actions: vec![
                    loom_domain::ControlActionKind::ApproveRequest,
                    loom_domain::ControlActionKind::RejectRequest,
                ],
                opened_reason: override_assessment.trigger_reason.clone(),
                opened_at: now_timestamp(),
                expires_at: None,
                supersedes: None,
            };
            self.store.save_pending_decision_window(&approval_window)?;
            let mut task = self
                .store
                .load_managed_task(&proposal.managed_task_ref)?
                .ok_or_else(|| {
                    LoomHarnessError::ManagedTaskNotFound(proposal.managed_task_ref.clone())
                })?;
            task.current_pending_window_ref = Some(approval_window.window_id.clone());
            task.updated_at = now_timestamp();
            self.store.save_managed_task(&task)?;
            self.store.enqueue_outbound(
                task.host_session_id.clone(),
                KernelOutboundPayload::ApprovalRequest(ApprovalRequestPayload {
                    managed_task_ref: proposal.managed_task_ref.clone(),
                    decision_token: approval_window.decision_token,
                    approval_scope: ApprovalScope::ToolExecution,
                    allowed_actions: vec![
                        ApprovalRequestAction::ApproveRequest,
                        ApprovalRequestAction::RejectRequest,
                    ],
                    why_now: override_assessment.trigger_reason.clone(),
                    risk_summary: format!(
                        "overall_risk_band={:?}",
                        override_assessment.overall_risk_band
                    ),
                    render_hint: RenderHint {
                        tone: RenderTone::Blocking,
                        emphasis: RenderEmphasis::Strong,
                        ..RenderHint::default()
                    },
                }),
            )?;
        } else {
            let task = self
                .store
                .load_managed_task(&proposal.managed_task_ref)?
                .ok_or_else(|| {
                    LoomHarnessError::ManagedTaskNotFound(proposal.managed_task_ref.clone())
                })?;
            self.store.enqueue_outbound(
                task.host_session_id,
                KernelOutboundPayload::ToolDecision(ToolDecisionPayload {
                    managed_task_ref: proposal.managed_task_ref.clone(),
                    decision_value: ToolDecisionValue::Allow,
                    decision_area: proposal.decision_area,
                    summary: override_assessment.trigger_reason.clone(),
                    render_hint: RenderHint::default(),
                }),
            )?;
        }

        Ok(override_assessment)
    }
}
