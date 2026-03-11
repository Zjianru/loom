use crate::support::{to_requirement_items, to_requirement_items as build_requirement_items};
use crate::{LoomHarness, LoomHarnessError};
use anyhow::{Result, anyhow};
use loom_domain::{
    ControlAction, ControlActionKind, KernelOutboundPayload, ManagedTask, PendingDecisionWindow,
    PendingDecisionWindowKind, PendingDecisionWindowStatus, RenderHint, RequirementOrigin,
    SemanticDecisionEnvelope, StartCardAction, StartCardPayload, TaskActivationReason,
    WorkflowStage, new_id, now_timestamp,
};

impl LoomHarness {
    pub fn ingest_semantic_decision(
        &self,
        decision: SemanticDecisionEnvelope,
    ) -> Result<Option<ManagedTask>> {
        match decision.interaction_lane {
            loom_domain::InteractionLane::Chat => Ok(None),
            loom_domain::InteractionLane::ManagedTaskCandidate => {
                self.open_candidate(decision).map(Some)
            }
            loom_domain::InteractionLane::ManagedTaskActive => {
                let managed_task_ref = decision
                    .managed_task_ref
                    .clone()
                    .ok_or(LoomHarnessError::MissingManagedTaskRefForActiveLane)?;
                let _task = self
                    .store
                    .load_managed_task(&managed_task_ref)?
                    .ok_or(LoomHarnessError::ManagedTaskNotFound(managed_task_ref))?;
                Ok(None)
            }
        }
    }

    fn open_candidate(&self, decision: SemanticDecisionEnvelope) -> Result<ManagedTask> {
        let Some(managed_task_class) = decision.managed_task_class.clone() else {
            return Err(LoomHarnessError::MissingManagedTaskSemantics.into());
        };
        let Some(work_horizon) = decision.work_horizon.clone() else {
            return Err(LoomHarnessError::MissingManagedTaskSemantics.into());
        };
        let activation_reason = decision
            .task_activation_reason
            .clone()
            .unwrap_or(TaskActivationReason::ExplicitUserRequest);
        let managed_task_ref = decision
            .managed_task_ref
            .clone()
            .unwrap_or_else(|| new_id("task"));
        let title = decision
            .title
            .clone()
            .unwrap_or_else(|| "Managed task".to_string());
        let summary = decision
            .summary
            .clone()
            .unwrap_or_else(|| decision.title.unwrap_or_else(|| "Managed task".to_string()));
        let expected_outcome = decision
            .expected_outcome
            .clone()
            .unwrap_or_else(|| "Deliver the requested coding change".to_string());
        let requirement_items = to_requirement_items(
            &decision.requirement_items,
            RequirementOrigin::InitialDecision,
        );

        let mut task = ManagedTask {
            managed_task_ref: managed_task_ref.clone(),
            host_session_id: decision.host_session_id.clone(),
            managed_task_class: managed_task_class.clone(),
            work_horizon: work_horizon.clone(),
            activation_reason: activation_reason.clone(),
            workflow_stage: WorkflowStage::Candidate,
            title: title.clone(),
            summary: summary.clone(),
            expected_outcome: expected_outcome.clone(),
            workspace_ref: decision.workspace_ref.clone(),
            repo_ref: decision.repo_ref.clone(),
            allowed_roots: decision.allowed_roots.clone(),
            secret_classes: decision.secret_classes.clone(),
            requirement_items,
            current_scope_ref: None,
            current_scope_version: None,
            current_baseline_risk_ref: None,
            current_execution_authorization_ref: None,
            current_pending_window_ref: None,
            active_run_ref: None,
            spec_bundle: None,
            phase_plan: None,
            agent_binding: None,
            review_result: None,
            proof_of_work_bundle: None,
            result_contract: None,
            created_at: now_timestamp(),
            updated_at: now_timestamp(),
        };
        self.store.save_managed_task(&task)?;
        self.reopen_candidate_window(&mut task, None)?;
        self.store.save_managed_task(&task)?;
        Ok(task)
    }

    pub(crate) fn reopen_candidate_window(
        &self,
        task: &mut ManagedTask,
        supersedes: Option<String>,
    ) -> Result<()> {
        let window = PendingDecisionWindow {
            window_id: new_id("window"),
            managed_task_ref: task.managed_task_ref.clone(),
            kind: PendingDecisionWindowKind::StartCandidate,
            decision_token: new_id("decision"),
            status: PendingDecisionWindowStatus::Open,
            allowed_actions: vec![
                ControlActionKind::ApproveStart,
                ControlActionKind::ModifyCandidate,
                ControlActionKind::CancelCandidate,
            ],
            opened_reason: "managed task candidate requires explicit start approval".into(),
            opened_at: now_timestamp(),
            expires_at: None,
            supersedes,
        };

        task.current_pending_window_ref = Some(window.window_id.clone());
        task.updated_at = now_timestamp();
        self.store.save_pending_decision_window(&window)?;
        self.store.enqueue_outbound(
            task.host_session_id.clone(),
            KernelOutboundPayload::StartCard(StartCardPayload {
                managed_task_ref: task.managed_task_ref.clone(),
                decision_token: window.decision_token.clone(),
                managed_task_class: task.managed_task_class.clone(),
                work_horizon: task.work_horizon.clone(),
                task_activation_reason: task.activation_reason.clone(),
                title: task.title.clone(),
                summary: task.summary.clone(),
                expected_outcome: task.expected_outcome.clone(),
                recommended_pack_ref: Some("coding_pack".into()),
                allowed_actions: vec![
                    StartCardAction::ApproveStart,
                    StartCardAction::ModifyCandidate,
                    StartCardAction::CancelCandidate,
                ],
                render_hint: RenderHint::default(),
            }),
        )?;
        Ok(())
    }

    pub(crate) fn modify_candidate(&self, action: ControlAction) -> Result<()> {
        let decision_token = action
            .decision_token
            .ok_or(LoomHarnessError::MissingDecisionToken)?;
        let window = self
            .store
            .find_open_window_by_token(&decision_token)?
            .ok_or(LoomHarnessError::StaleDecisionToken)?;
        let managed_task_ref = action
            .managed_task_ref
            .clone()
            .unwrap_or(window.managed_task_ref.clone());
        let mut task = self
            .store
            .load_managed_task(&managed_task_ref)?
            .ok_or_else(|| LoomHarnessError::ManagedTaskNotFound(managed_task_ref.clone()))?;

        if let Some(title) = action.payload.title.clone() {
            task.title = title;
        }
        if let Some(summary) = action.payload.summary.clone() {
            task.summary = summary;
        }
        if let Some(expected_outcome) = action.payload.expected_outcome.clone() {
            task.expected_outcome = expected_outcome;
        }
        if !action.payload.allowed_roots.is_empty() {
            task.allowed_roots = action.payload.allowed_roots.clone();
        }
        if !action.payload.secret_classes.is_empty() {
            task.secret_classes = action.payload.secret_classes.clone();
        }
        if !action.payload.requirement_items.is_empty() {
            task.requirement_items.extend(build_requirement_items(
                &action.payload.requirement_items,
                RequirementOrigin::TaskChange,
            ));
        }
        task.workspace_ref = action.payload.workspace_ref.clone().or(task.workspace_ref.clone());
        task.repo_ref = action.payload.repo_ref.clone().or(task.repo_ref.clone());
        self.store
            .update_window_status(&window.window_id, PendingDecisionWindowStatus::Consumed)?;
        self.reopen_candidate_window(&mut task, Some(window.window_id.clone()))?;
        self.store.save_managed_task(&task)?;
        Ok(())
    }

    pub(crate) fn cancel_candidate(&self, action: ControlAction) -> Result<()> {
        let decision_token = action
            .decision_token
            .ok_or(LoomHarnessError::MissingDecisionToken)?;
        let window = self
            .store
            .find_open_window_by_token(&decision_token)?
            .ok_or(LoomHarnessError::StaleDecisionToken)?;
        let managed_task_ref = action
            .managed_task_ref
            .clone()
            .unwrap_or(window.managed_task_ref.clone());
        let mut task = self
            .store
            .load_managed_task(&managed_task_ref)?
            .ok_or_else(|| anyhow!("managed task not found: {managed_task_ref}"))?;
        self.store
            .update_window_status(&window.window_id, PendingDecisionWindowStatus::Cancelled)?;
        task.workflow_stage = WorkflowStage::Closed;
        task.current_pending_window_ref = None;
        task.updated_at = now_timestamp();
        self.store.save_managed_task(&task)?;
        Ok(())
    }
}
