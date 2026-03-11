use crate::{LoomHarness, LoomHarnessError};
use anyhow::{Result, anyhow};
use loom_approval::issue_execution_authorization;
use loom_domain::ControlAction;
use serde_json::json;

impl LoomHarness {
    pub(crate) fn resume_task(&self, action: ControlAction) -> Result<()> {
        let managed_task_ref = action
            .managed_task_ref
            .clone()
            .ok_or_else(|| anyhow!("resume_task requires managed_task_ref"))?;
        let task = self
            .store
            .load_managed_task(&managed_task_ref)?
            .ok_or_else(|| LoomHarnessError::ManagedTaskNotFound(managed_task_ref.clone()))?;
        let scope = self
            .store
            .latest_scope_snapshot(&managed_task_ref)?
            .ok_or_else(|| anyhow!("scope missing for task {}", managed_task_ref))?;
        let baseline = self
            .store
            .latest_task_baseline(&managed_task_ref)?
            .ok_or_else(|| anyhow!("baseline missing for task {}", managed_task_ref))?;
        let capability = self
            .store
            .latest_capability_snapshot(&task.host_session_id)?
            .ok_or_else(|| {
                LoomHarnessError::MissingCapabilitySnapshot(task.host_session_id.clone())
            })?;
        let run_ref = task
            .active_run_ref
            .clone()
            .ok_or_else(|| anyhow!("run missing for task {}", managed_task_ref))?;
        let run = self
            .store
            .load_task_run(&run_ref)?
            .ok_or_else(|| anyhow!("run payload missing for task {}", managed_task_ref))?;
        let previous_auth =
            task.current_execution_authorization_ref
                .clone()
                .and_then(|authorization_id| {
                    self.store
                        .load_execution_authorization(&authorization_id)
                        .ok()
                        .flatten()
                });
        let previous_auth_id = previous_auth
            .as_ref()
            .map(|authorization| authorization.authorization_id.clone());
        if let Some(ref previous_auth) = previous_auth {
            self.store.update_authorization_status(
                &previous_auth.authorization_id,
                loom_domain::ExecutionAuthorizationStatus::Superseded,
            )?;
        }
        let auth = issue_execution_authorization(
            managed_task_ref.clone(),
            &run,
            &capability,
            &scope,
            &baseline,
            "resume_task reissue",
            previous_auth_id,
            false,
        );
        self.store.save_execution_authorization(&auth)?;
        self.log_event(
            &managed_task_ref,
            "execution_authorization.reissued",
            json!({
                "managed_task_ref": managed_task_ref,
                "authorization_id": auth.authorization_id,
                "overall_risk_band": baseline.overall_risk_band,
                "scope_version": scope.scope_version,
                "derived_consequences": baseline.derived_consequences,
                "supersedes": auth.supersedes,
                "capability_snapshot_ref": auth.capability_snapshot_ref,
            }),
        )?;
        Ok(())
    }
}
