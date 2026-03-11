use crate::support::to_requirement_items;
use crate::{LoomHarness, LoomHarnessError};
use anyhow::{Result, anyhow};
use loom_approval::issue_execution_authorization;
use loom_domain::{ControlAction, TaskScopeSnapshot, new_id, now_timestamp};
use loom_risk::assess_task_baseline;
use serde_json::json;

impl LoomHarness {
    pub(crate) fn request_task_change(&self, action: ControlAction) -> Result<()> {
        let managed_task_ref = action
            .managed_task_ref
            .clone()
            .ok_or_else(|| anyhow!("request_task_change requires managed_task_ref"))?;
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
        let previous_scope = self
            .store
            .latest_scope_snapshot(&managed_task_ref)?
            .ok_or_else(|| anyhow!("scope missing for task {}", managed_task_ref))?;
        let previous_baseline = self
            .store
            .latest_task_baseline(&managed_task_ref)?
            .ok_or_else(|| anyhow!("baseline missing for task {}", managed_task_ref))?;
        let previous_auth = task
            .current_execution_authorization_ref
            .clone()
            .and_then(|authorization_id| {
                self.store
                    .load_execution_authorization(&authorization_id)
                    .ok()
                    .flatten()
            })
            .ok_or_else(|| anyhow!("authorization missing for task {}", managed_task_ref))?;
        let run_ref = task
            .active_run_ref
            .clone()
            .ok_or_else(|| anyhow!("run missing for task {}", managed_task_ref))?;
        let run = self
            .store
            .load_task_run(&run_ref)?
            .ok_or_else(|| anyhow!("run payload missing for task {}", managed_task_ref))?;

        let new_scope_version = previous_scope.scope_version + 1;
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

        let scope = TaskScopeSnapshot {
            scope_id: new_id("scope"),
            managed_task_ref: managed_task_ref.clone(),
            scope_version: new_scope_version,
            scope_summary: task.summary.clone(),
            requirement_items: task.requirement_items.clone(),
            workspace_ref: action
                .payload
                .workspace_ref
                .clone()
                .or(task.workspace_ref.clone()),
            repo_ref: action.payload.repo_ref.clone().or(task.repo_ref.clone()),
            allowed_roots: task.allowed_roots.clone(),
            secret_classes: task.secret_classes.clone(),
            constraints: previous_scope.constraints.clone(),
            assumptions: previous_scope.assumptions.clone(),
            source_decision_ref: action
                .source_decision_ref
                .clone()
                .unwrap_or_else(|| new_id("decision-ref")),
            created_at: now_timestamp(),
        };
        self.store.save_scope_snapshot(&scope)?;

        let new_baseline = assess_task_baseline(
            &task,
            &scope,
            &capability,
            "request_task_change accepted",
            Some(previous_baseline.assessment_id.clone()),
        );
        self.store.save_risk_assessment(&new_baseline)?;
        self.log_event(
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
        self.log_event(
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

        self.store.update_authorization_status(
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
        self.store.save_execution_authorization(&auth)?;
        self.log_event(
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
        self.store.save_managed_task(&task)?;
        Ok(())
    }
}
