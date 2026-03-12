use crate::support::to_requirement_items;
use crate::{LoomHarness, LoomHarnessError};
use anyhow::{Result, anyhow};
use loom_approval::issue_execution_authorization;
use loom_domain::{
    ControlAction, ExecutionAuthorization, HostCapabilitySnapshot, IsolatedTaskRun, ManagedTask,
    RiskAssessment, SemanticDecisionKind, SemanticDecisionPayload, TaskScopeSnapshot, new_id,
    now_timestamp,
};
use loom_risk::assess_task_baseline;
use loom_store::LoomStoreTx;
use serde_json::json;

struct RequestTaskChangeContext {
    managed_task_ref: String,
    source_decision_ref: String,
    task: ManagedTask,
    capability: HostCapabilitySnapshot,
    previous_scope: TaskScopeSnapshot,
    previous_baseline: RiskAssessment,
    previous_auth: ExecutionAuthorization,
    run: IsolatedTaskRun,
}

pub(crate) fn is_task_change_clarification_error(error: &anyhow::Error) -> bool {
    matches!(
        error.downcast_ref::<LoomHarnessError>(),
        Some(
            LoomHarnessError::MissingTaskChangeSourceDecisionRef
                | LoomHarnessError::TaskChangeSourceDecisionNotFound(_)
                | LoomHarnessError::InvalidTaskChangeSourceDecision
                | LoomHarnessError::TaskChangeManagedTaskMismatch
        )
    )
}

impl LoomHarness {
    pub(crate) fn request_task_change(&self, action: ControlAction) -> Result<()> {
        let known_task_ref = action.managed_task_ref.clone();
        let mut tx = self.store.begin_tx()?;
        match self.request_task_change_tx(&mut tx, action) {
            Ok(()) => tx.commit(),
            Err(error) => {
                if is_task_change_clarification_error(&error) {
                    if let Some(task_ref) = known_task_ref.as_ref() {
                        self.request_task_change_clarification_tx(
                            &mut tx,
                            task_ref,
                            &error.to_string(),
                        )?;
                        tx.commit()?;
                    }
                }
                Err(error)
            }
        }
    }

    fn validate_request_task_change_tx(
        &self,
        tx: &mut LoomStoreTx<'_>,
        action: &ControlAction,
    ) -> Result<RequestTaskChangeContext> {
        let source_decision_ref = action
            .source_decision_ref
            .clone()
            .ok_or(LoomHarnessError::MissingTaskChangeSourceDecisionRef)?;
        let source_decision = tx
            .load_semantic_decision(&source_decision_ref)?
            .ok_or_else(|| {
                LoomHarnessError::TaskChangeSourceDecisionNotFound(source_decision_ref.clone())
            })?;
        if source_decision.decision_kind != SemanticDecisionKind::TaskChange
            || !matches!(
                source_decision.decision_payload,
                SemanticDecisionPayload::TaskChange(_)
            )
        {
            return Err(LoomHarnessError::InvalidTaskChangeSourceDecision.into());
        }
        let managed_task_ref = action
            .managed_task_ref
            .clone()
            .or(source_decision.managed_task_ref.clone())
            .ok_or_else(|| anyhow!("request_task_change requires managed_task_ref"))?;
        if let Some(source_task_ref) = source_decision.managed_task_ref.as_ref() {
            if source_task_ref != &managed_task_ref {
                return Err(LoomHarnessError::TaskChangeManagedTaskMismatch.into());
            }
        }

        let task = tx
            .load_managed_task(&managed_task_ref)?
            .ok_or_else(|| LoomHarnessError::ManagedTaskNotFound(managed_task_ref.clone()))?;
        let capability = tx
            .latest_capability_snapshot(&task.host_session_id)?
            .ok_or_else(|| {
                LoomHarnessError::MissingCapabilitySnapshot(task.host_session_id.clone())
            })?;
        let previous_scope = tx
            .latest_scope_snapshot(&managed_task_ref)?
            .ok_or_else(|| anyhow!("scope missing for task {}", managed_task_ref))?;
        let previous_baseline = tx
            .latest_task_baseline(&managed_task_ref)?
            .ok_or_else(|| anyhow!("baseline missing for task {}", managed_task_ref))?;
        let previous_auth = task
            .current_execution_authorization_ref
            .clone()
            .ok_or_else(|| anyhow!("authorization missing for task {}", managed_task_ref))
            .and_then(|authorization_id| {
                tx.load_execution_authorization(&authorization_id)?
                    .ok_or_else(|| anyhow!("authorization missing for task {}", managed_task_ref))
            })?;
        let run_ref = task
            .active_run_ref
            .clone()
            .ok_or_else(|| anyhow!("run missing for task {}", managed_task_ref))?;
        let run = tx
            .load_task_run(&run_ref)?
            .ok_or_else(|| anyhow!("run payload missing for task {}", managed_task_ref))?;

        Ok(RequestTaskChangeContext {
            managed_task_ref,
            source_decision_ref,
            task,
            capability,
            previous_scope,
            previous_baseline,
            previous_auth,
            run,
        })
    }

    pub(crate) fn request_task_change_tx(
        &self,
        tx: &mut LoomStoreTx<'_>,
        action: ControlAction,
    ) -> Result<()> {
        let RequestTaskChangeContext {
            managed_task_ref,
            source_decision_ref,
            mut task,
            capability,
            previous_scope,
            previous_baseline,
            previous_auth,
            run,
        } = self.validate_request_task_change_tx(tx, &action)?;

        self.log_event_tx(
            tx,
            &managed_task_ref,
            "control_action_received",
            json!({
                "managed_task_ref": managed_task_ref.clone(),
                "action_id": action.action_id.clone(),
                "action_kind": action.kind.clone(),
                "actor": action.actor.clone(),
                "source_decision_ref": source_decision_ref.clone(),
                "payload": {
                    "title": action.payload.title.clone(),
                    "summary": action.payload.summary.clone(),
                    "expected_outcome": action.payload.expected_outcome.clone(),
                    "workspace_ref": action.payload.workspace_ref.clone(),
                    "repo_ref": action.payload.repo_ref.clone(),
                    "allowed_roots": action.payload.allowed_roots.clone(),
                    "secret_classes": action.payload.secret_classes.clone(),
                    "requirement_items": action.payload.requirement_items.clone(),
                    "rationale": action.payload.rationale.clone(),
                },
            }),
        )?;

        let new_scope_version = previous_scope.scope_version + 1;
        if let Some(title) = action.payload.title.clone() {
            task.title = title;
        }
        task.summary = action
            .payload
            .summary
            .clone()
            .unwrap_or_else(|| task.summary.clone());
        task.expected_outcome = action
            .payload
            .expected_outcome
            .clone()
            .unwrap_or_else(|| task.expected_outcome.clone());
        if !action.payload.allowed_roots.is_empty() {
            task.allowed_roots = action.payload.allowed_roots.clone();
        }
        if !action.payload.secret_classes.is_empty() {
            task.secret_classes = action.payload.secret_classes.clone();
        }
        if !action.payload.requirement_items.is_empty() {
            task.requirement_items.extend(to_requirement_items(
                &action.payload.requirement_items,
                loom_domain::RequirementOrigin::TaskChange,
            ));
        }
        task.workspace_ref = action
            .payload
            .workspace_ref
            .clone()
            .or(task.workspace_ref.clone());
        task.repo_ref = action.payload.repo_ref.clone().or(task.repo_ref.clone());

        self.log_event_tx(
            tx,
            &managed_task_ref,
            "task_change_requested",
            json!({
                "managed_task_ref": managed_task_ref.clone(),
                "action_id": action.action_id.clone(),
                "source_decision_ref": source_decision_ref.clone(),
                "requested_title": action.payload.title.clone(),
                "requested_summary": action.payload.summary.clone(),
                "requested_expected_outcome": action.payload.expected_outcome.clone(),
                "requested_workspace_ref": action.payload.workspace_ref.clone(),
                "requested_repo_ref": action.payload.repo_ref.clone(),
                "requested_allowed_roots": action.payload.allowed_roots.clone(),
                "requested_secret_classes": action.payload.secret_classes.clone(),
                "requested_requirement_items": action.payload.requirement_items.clone(),
                "rationale": action.payload.rationale.clone(),
            }),
        )?;

        let scope = TaskScopeSnapshot {
            scope_id: new_id("scope"),
            managed_task_ref: managed_task_ref.clone(),
            scope_version: new_scope_version,
            scope_summary: task.summary.clone(),
            requirement_items: task.requirement_items.clone(),
            workspace_ref: task.workspace_ref.clone(),
            repo_ref: task.repo_ref.clone(),
            allowed_roots: task.allowed_roots.clone(),
            secret_classes: task.secret_classes.clone(),
            constraints: previous_scope.constraints.clone(),
            assumptions: previous_scope.assumptions.clone(),
            source_decision_ref: source_decision_ref.clone(),
            created_at: now_timestamp(),
        };
        tx.save_scope_snapshot(&scope)?;
        self.log_event_tx(
            tx,
            &managed_task_ref,
            "task_scope_revised",
            json!({
                "managed_task_ref": managed_task_ref.clone(),
                "action_id": action.action_id.clone(),
                "previous_scope_id": previous_scope.scope_id,
                "previous_scope_version": previous_scope.scope_version,
                "scope_id": scope.scope_id.clone(),
                "scope_version": scope.scope_version,
                "workspace_ref": scope.workspace_ref.clone(),
                "repo_ref": scope.repo_ref.clone(),
                "allowed_roots": scope.allowed_roots.clone(),
                "secret_classes": scope.secret_classes.clone(),
                "source_decision_ref": scope.source_decision_ref.clone(),
            }),
        )?;

        let new_baseline = assess_task_baseline(
            &task,
            &scope,
            &capability,
            "request_task_change accepted",
            Some(previous_baseline.assessment_id.clone()),
        );
        tx.save_risk_assessment(&new_baseline)?;
        self.log_event_tx(
            tx,
            &managed_task_ref,
            "risk_assessment.created",
            json!({
                "managed_task_ref": managed_task_ref,
                "assessment_id": new_baseline.assessment_id,
                "overall_risk_band": new_baseline.overall_risk_band,
                "scope_version": scope.scope_version,
                "derived_consequences": new_baseline.derived_consequences,
                "supersedes": new_baseline.supersedes,
                "capability_snapshot_ref": new_baseline.capability_snapshot_ref,
            }),
        )?;
        self.log_event_tx(
            tx,
            &managed_task_ref,
            "risk_assessment.superseded",
            json!({
                "managed_task_ref": managed_task_ref,
                "superseded": previous_baseline.assessment_id,
                "replacement": new_baseline.assessment_id,
                "overall_risk_band": new_baseline.overall_risk_band,
                "scope_version": scope.scope_version,
                "derived_consequences": new_baseline.derived_consequences,
                "capability_snapshot_ref": new_baseline.capability_snapshot_ref,
            }),
        )?;

        tx.update_authorization_status(
            &previous_auth.authorization_id,
            loom_domain::ExecutionAuthorizationStatus::Superseded,
        )?;
        let auth = issue_execution_authorization(
            managed_task_ref.clone(),
            &run,
            &capability,
            &scope,
            &new_baseline,
            "scope change reissue",
            Some(previous_auth.authorization_id),
            false,
        );
        tx.save_execution_authorization(&auth)?;
        self.log_event_tx(
            tx,
            &managed_task_ref,
            "execution_authorization.reissued",
            json!({
                "managed_task_ref": managed_task_ref,
                "authorization_id": auth.authorization_id,
                "overall_risk_band": new_baseline.overall_risk_band,
                "scope_version": scope.scope_version,
                "derived_consequences": new_baseline.derived_consequences,
                "supersedes": auth.supersedes,
                "capability_snapshot_ref": auth.capability_snapshot_ref,
            }),
        )?;

        task.current_scope_ref = Some(scope.scope_id);
        task.current_scope_version = Some(scope.scope_version);
        task.current_baseline_risk_ref = Some(new_baseline.assessment_id);
        task.current_execution_authorization_ref = Some(auth.authorization_id);
        task.updated_at = now_timestamp();
        tx.save_managed_task(&task)?;
        Ok(())
    }
}
