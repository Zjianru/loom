use crate::LoomHarness;
use anyhow::{Result, anyhow};
use loom_approval::issue_execution_authorization;
use loom_domain::{HostCapabilitySnapshot, now_timestamp};
use loom_risk::assess_task_baseline;
use serde_json::json;

impl LoomHarness {
    pub fn ingest_capability_snapshot(&self, snapshot: HostCapabilitySnapshot) -> Result<()> {
        let latest_before = self
            .store
            .latest_capability_snapshot(&snapshot.host_session_id)?;
        self.store.save_capability_snapshot(&snapshot)?;

        if let Some(mut task) = self
            .store
            .find_active_task_for_session(&snapshot.host_session_id)?
        {
            let Some(current_scope) = self.store.latest_scope_snapshot(&task.managed_task_ref)?
            else {
                return Ok(());
            };
            let Some(current_run_ref) = task.active_run_ref.clone() else {
                return Ok(());
            };
            let current_run = self
                .store
                .load_task_run(&current_run_ref)?
                .ok_or_else(|| anyhow!("active run missing for task {}", task.managed_task_ref))?;
            let previous_auth =
                task.current_execution_authorization_ref
                    .clone()
                    .and_then(|auth_id| {
                        self.store
                            .load_execution_authorization(&auth_id)
                            .ok()
                            .flatten()
                    });

            let should_reissue = match latest_before {
                None => false,
                Some(before) => {
                    before.allowed_tools != snapshot.allowed_tools
                        || before.readable_roots != snapshot.readable_roots
                        || before.writable_roots != snapshot.writable_roots
                        || before.secret_classes != snapshot.secret_classes
                        || before.supports_spawn_agents != snapshot.supports_spawn_agents
                        || before.available_agent_ids != snapshot.available_agent_ids
                }
            };
            if !should_reissue {
                return Ok(());
            }

            let previous_baseline = self
                .store
                .latest_task_baseline(&task.managed_task_ref)?
                .ok_or_else(|| anyhow!("baseline missing for task {}", task.managed_task_ref))?;
            let new_baseline = assess_task_baseline(
                &task,
                &current_scope,
                &snapshot,
                "capability drift requires baseline refresh",
                Some(previous_baseline.assessment_id.clone()),
            );
            self.store.save_risk_assessment(&new_baseline)?;
            self.log_event(
                &task.managed_task_ref,
                "risk_assessment.created",
                json!(new_baseline),
            )?;
            self.log_event(
                &task.managed_task_ref,
                "risk_assessment.superseded",
                json!({
                    "managed_task_ref": task.managed_task_ref,
                    "superseded": previous_baseline.assessment_id,
                    "replacement": new_baseline.assessment_id,
                    "overall_risk_band": new_baseline.overall_risk_band,
                    "scope_version": current_scope.scope_version,
                    "derived_consequences": new_baseline.derived_consequences,
                    "capability_snapshot_ref": snapshot.capability_snapshot_ref,
                }),
            )?;
            let previous_auth_id = previous_auth
                .as_ref()
                .map(|auth| auth.authorization_id.clone());
            if let Some(ref previous_auth) = previous_auth {
                self.store.update_authorization_status(
                    &previous_auth.authorization_id,
                    loom_domain::ExecutionAuthorizationStatus::Superseded,
                )?;
            }
            let auth = issue_execution_authorization(
                task.managed_task_ref.clone(),
                &current_run,
                &snapshot,
                &current_scope,
                &new_baseline,
                "capability drift reissue",
                previous_auth_id,
                false,
            );
            self.store.save_execution_authorization(&auth)?;
            self.log_event(
                &task.managed_task_ref,
                "execution_authorization.reissued",
                json!({
                    "managed_task_ref": task.managed_task_ref,
                    "authorization_id": auth.authorization_id,
                    "overall_risk_band": new_baseline.overall_risk_band,
                    "scope_version": current_scope.scope_version,
                    "derived_consequences": new_baseline.derived_consequences,
                    "supersedes": auth.supersedes,
                    "capability_snapshot_ref": snapshot.capability_snapshot_ref,
                }),
            )?;
            task.current_baseline_risk_ref = Some(new_baseline.assessment_id);
            task.current_execution_authorization_ref = Some(auth.authorization_id);
            task.updated_at = now_timestamp();
            self.store.save_managed_task(&task)?;
        }

        Ok(())
    }
}
