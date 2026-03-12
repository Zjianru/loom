use crate::support::{
    build_agent_binding, build_default_phase_plan, build_spec_bundle, build_status_notice,
    build_worker_command, host_agent_for_role,
};
use crate::{LoomHarness, LoomHarnessError};
use anyhow::Result;
use loom_approval::issue_execution_authorization;
use loom_domain::{
    AcceptanceResult, AgentRoleKind, ControlAction, HostCapabilitySnapshot, IsolatedTaskRun,
    IsolatedTaskRunStatus, KernelOutboundPayload, ReviewResult, ReviewSummary, ReviewVerdict,
    StatusNoticeKind, TaskScopeSnapshot, WorkflowStage, new_id, now_timestamp,
};
use loom_risk::assess_task_baseline;
use loom_store::LoomStoreTx;
use serde_json::json;

fn legacy_candidate_source_decision_ref(window_id: &str) -> String {
    format!("legacy-candidate-window:{window_id}")
}

fn resolve_initial_scope_source_decision_ref(
    action: &ControlAction,
    window: &loom_domain::PendingDecisionWindow,
    task: &loom_domain::ManagedTask,
) -> Result<String> {
    if let Some(source_decision_ref) = action
        .source_decision_ref
        .clone()
        .or(window.source_decision_ref.clone())
    {
        return Ok(source_decision_ref);
    }
    if window.kind == loom_domain::PendingDecisionWindowKind::StartCandidate
        && task.workflow_stage == WorkflowStage::Candidate
        && task.current_scope_ref.is_none()
    {
        return Ok(legacy_candidate_source_decision_ref(&window.window_id));
    }
    Err(LoomHarnessError::MissingScopeSourceDecisionRef.into())
}

impl LoomHarness {
    pub(crate) fn approve_start(&self, action: ControlAction) -> Result<()> {
        let mut tx = self.store.begin_tx()?;
        self.approve_start_tx(&mut tx, action)?;
        tx.commit()
    }

    pub(crate) fn approve_start_tx(
        &self,
        tx: &mut LoomStoreTx<'_>,
        action: ControlAction,
    ) -> Result<()> {
        let decision_token = action
            .decision_token
            .as_ref()
            .cloned()
            .ok_or(LoomHarnessError::MissingDecisionToken)?;
        let window = tx
            .find_open_window_by_token(&decision_token)?
            .ok_or(LoomHarnessError::StaleDecisionToken)?;
        let managed_task_ref = action
            .managed_task_ref
            .clone()
            .unwrap_or(window.managed_task_ref.clone());
        let mut task = tx
            .load_managed_task(&managed_task_ref)?
            .ok_or_else(|| LoomHarnessError::ManagedTaskNotFound(managed_task_ref.clone()))?;
        let capability = tx
            .latest_capability_snapshot(&task.host_session_id)?
            .ok_or_else(|| {
                LoomHarnessError::MissingCapabilitySnapshot(task.host_session_id.clone())
            })?;

        tx.update_window_status(
            &window.window_id,
            loom_domain::PendingDecisionWindowStatus::Consumed,
        )?;

        let scope = TaskScopeSnapshot {
            scope_id: new_id("scope"),
            managed_task_ref: managed_task_ref.clone(),
            scope_version: 1,
            scope_summary: task.summary.clone(),
            requirement_items: task.requirement_items.clone(),
            workspace_ref: task.workspace_ref.clone(),
            repo_ref: task.repo_ref.clone(),
            allowed_roots: task.allowed_roots.clone(),
            secret_classes: task.secret_classes.clone(),
            constraints: vec![],
            assumptions: vec![],
            source_decision_ref: resolve_initial_scope_source_decision_ref(&action, &window, &task)?,
            created_at: now_timestamp(),
        };
        tx.save_scope_snapshot(&scope)?;
        self.log_event_tx(
            tx,
            &managed_task_ref,
            "task_scope_snapshot.created",
            json!({
                "managed_task_ref": managed_task_ref,
                "scope_id": scope.scope_id,
                "scope_version": scope.scope_version,
            }),
        )?;

        let spec_bundle = build_spec_bundle(&task, &scope);
        let mut run = IsolatedTaskRun {
            run_ref: new_id("run"),
            managed_task_ref: managed_task_ref.clone(),
            status: IsolatedTaskRunStatus::Active,
            spec_bundle: spec_bundle.clone(),
            created_at: now_timestamp(),
            updated_at: now_timestamp(),
        };
        let phase_plan = build_default_phase_plan(&managed_task_ref);
        let binding = build_agent_binding(&task, &run.run_ref, &capability.capability_snapshot_ref);
        let baseline = assess_task_baseline(
            &task,
            &scope,
            &capability,
            "approve_start baseline issued",
            None,
        );
        tx.save_risk_assessment(&baseline)?;
        self.log_event_tx(
            tx,
            &managed_task_ref,
            "risk_assessment.created",
            json!({
                "managed_task_ref": managed_task_ref,
                "assessment_id": baseline.assessment_id,
                "overall_risk_band": baseline.overall_risk_band,
                "scope_version": scope.scope_version,
                "derived_consequences": baseline.derived_consequences,
                "supersedes": baseline.supersedes,
                "capability_snapshot_ref": baseline.capability_snapshot_ref,
            }),
        )?;

        let auth = issue_execution_authorization(
            managed_task_ref.clone(),
            &run,
            &capability,
            &scope,
            &baseline,
            "approve_start execution authorization",
            None,
            false,
        );
        tx.save_task_run(&run)?;
        tx.save_phase_plan(&phase_plan)?;
        tx.save_agent_binding(&binding)?;
        tx.save_execution_authorization(&auth)?;
        self.log_event_tx(
            tx,
            &managed_task_ref,
            "execution_authorization.issued",
            json!({
                "managed_task_ref": managed_task_ref,
                "authorization_id": auth.authorization_id,
                "overall_risk_band": baseline.overall_risk_band,
                "scope_version": scope.scope_version,
                "derived_consequences": baseline.derived_consequences,
                "supersedes": auth.supersedes,
                "capability_snapshot_ref": auth.capability_snapshot_ref,
                "spawn_agent_allowed": auth
                    .granted_areas
                    .iter()
                    .find(|area| area.decision_area == loom_domain::DecisionArea::TaskExecution)
                    .map(|area| area.spawn_agent_allowed)
                    .unwrap_or(false),
            }),
        )?;

        task.current_scope_ref = Some(scope.scope_id.clone());
        task.current_scope_version = Some(scope.scope_version);
        task.current_baseline_risk_ref = Some(baseline.assessment_id.clone());
        task.current_execution_authorization_ref = Some(auth.authorization_id.clone());
        task.current_pending_window_ref = None;
        task.active_run_ref = Some(run.run_ref.clone());
        task.spec_bundle = Some(spec_bundle.clone());
        task.phase_plan = Some(phase_plan.clone());
        task.agent_binding = Some(binding.clone());
        task.updated_at = now_timestamp();

        if capability_supports_worker_roundtrip(&capability) {
            let worker_command = build_worker_command(&task, &run.run_ref, &binding, &spec_bundle);
            tx.enqueue_host_execution_command(&worker_command)?;
            self.log_event_tx(
                tx,
                &managed_task_ref,
                "host_execution_command_queued",
                json!({
                    "managed_task_ref": managed_task_ref,
                    "command_id": worker_command.command_id,
                    "binding_id": worker_command.binding_id,
                    "role_kind": worker_command.role_kind,
                    "host_session_id": worker_command.host_session_id,
                    "host_agent_id": host_agent_for_role(&binding, AgentRoleKind::Worker),
                    "host_child_execution_ref": serde_json::Value::Null,
                    "host_child_run_ref": serde_json::Value::Null,
                    "artifact_refs": [],
                    "terminal_status": serde_json::Value::Null,
                }),
            )?;
            task.workflow_stage = WorkflowStage::Execute;
            tx.save_managed_task(&task)?;
            tx.enqueue_outbound(
                task.host_session_id.clone(),
                KernelOutboundPayload::StatusNotice(build_status_notice(
                    &managed_task_ref,
                    &phase_plan,
                    "execute",
                    StatusNoticeKind::StageEntered,
                    "Entered execute stage",
                    "Task entered execute and queued worker dispatch.",
                    Some("Execution authorization is active and worker dispatch has been queued."),
                )?),
            )?;
            return Ok(());
        }

        let blocked_review = ReviewResult {
            review_result_id: new_id("review"),
            managed_task_ref: task.managed_task_ref.clone(),
            run_ref: run.run_ref.clone(),
            reviewer_group_ref: crate::support::SYSTEM_REVIEW_GROUP_REF.into(),
            review_verdict: ReviewVerdict::Blocked,
            findings: vec![
                "host capability snapshot does not prove sessions_spawn plus required agents".into(),
            ],
            review_artifacts: Vec::new(),
            summary: ReviewSummary {
                review_verdict: ReviewVerdict::Blocked,
                summary: "Execution was blocked because the host bridge cannot spawn the required worker/recorder agents.".into(),
                key_findings: vec![
                    "supports_spawn_agents=false or required host agents are unavailable".into(),
                ],
                follow_up_required: true,
            },
            reviewed_at: now_timestamp(),
        };
        tx.enqueue_outbound(
            task.host_session_id.clone(),
            KernelOutboundPayload::StatusNotice(build_status_notice(
                &task.managed_task_ref,
                &phase_plan,
                "execute",
                StatusNoticeKind::Blocked,
                "Execute stage blocked",
                "Task could not enter execute because the host bridge cannot spawn the required worker/recorder agents.",
                Some("supports_spawn_agents=false or required host agents are unavailable."),
            )?),
        )?;
        self.finalize_task_result_tx(
            tx,
            &mut task,
            &mut run,
            &binding,
            blocked_review.summary.summary.clone(),
            Vec::new(),
            blocked_review,
            loom_domain::ResultOutcome::Blocked,
            AcceptanceResult::EscalatedToUser,
        )
    }
}

fn capability_supports_worker_roundtrip(capability: &HostCapabilitySnapshot) -> bool {
    capability.supports_spawn_agents
        && capability
            .available_agent_ids
            .iter()
            .any(|agent_id| agent_id == "coder")
        && capability
            .available_agent_ids
            .iter()
            .any(|agent_id| agent_id == "product_analyst")
}
