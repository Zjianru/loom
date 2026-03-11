use crate::support::{
    build_agent_binding, build_default_phase_plan, build_spec_bundle, build_worker_command,
    host_agent_for_role,
};
use crate::{LoomHarness, LoomHarnessError};
use anyhow::Result;
use loom_approval::issue_execution_authorization;
use loom_domain::{
    AcceptanceResult, AgentRoleKind, ControlAction, HostCapabilitySnapshot, IsolatedTaskRun,
    IsolatedTaskRunStatus, KernelOutboundPayload, ReviewResult, ReviewSummary, ReviewVerdict,
    TaskScopeSnapshot, ToolDecisionPayload, ToolDecisionValue, WorkflowStage, new_id,
    now_timestamp,
};
use loom_risk::assess_task_baseline;
use serde_json::json;

impl LoomHarness {
    pub(crate) fn approve_start(&self, action: ControlAction) -> Result<()> {
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
        let capability = self
            .store
            .latest_capability_snapshot(&task.host_session_id)?
            .ok_or_else(|| {
                LoomHarnessError::MissingCapabilitySnapshot(task.host_session_id.clone())
            })?;

        self.store.update_window_status(
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
            source_decision_ref: action
                .source_decision_ref
                .clone()
                .unwrap_or_else(|| new_id("decision-ref")),
            created_at: now_timestamp(),
        };
        self.store.save_scope_snapshot(&scope)?;
        self.log_event(
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
        self.store.save_risk_assessment(&baseline)?;
        self.log_event(
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
        self.store.save_task_run(&run)?;
        self.store.save_phase_plan(&phase_plan)?;
        self.store.save_agent_binding(&binding)?;
        self.store.save_execution_authorization(&auth)?;
        self.log_event(
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
            self.store.enqueue_host_execution_command(&worker_command)?;
            self.log_event(
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
            self.store.save_managed_task(&task)?;
            self.store.enqueue_outbound(
                task.host_session_id.clone(),
                KernelOutboundPayload::ToolDecision(ToolDecisionPayload {
                    managed_task_ref: managed_task_ref.clone(),
                    decision_value: ToolDecisionValue::Allow,
                    decision_area: loom_domain::DecisionArea::TaskExecution,
                    summary: "Task entered execute stage with active authorization and queued worker dispatch."
                        .into(),
                    render_hint: loom_domain::RenderHint::default(),
                }),
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
        self.finalize_task_result(
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
