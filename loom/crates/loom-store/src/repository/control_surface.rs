use crate::LoomStore;
use anyhow::{Result, anyhow};
use loom_domain::{
    ControlSurfaceType, CurrentControlSurfaceProjection, HostSessionId, ManagedTask,
    PendingDecisionWindowKind, PendingDecisionWindowStatus,
};
use rusqlite::params;

impl LoomStore {
    pub fn read_current_control_surface(
        &self,
        host_session_id: &HostSessionId,
    ) -> Result<Option<CurrentControlSurfaceProjection>> {
        let tasks = self.load_json_rows::<ManagedTask>(
            "
            SELECT payload_json
            FROM managed_tasks
            WHERE host_session_id = ?1 AND current_pending_window_ref IS NOT NULL
            ",
            params![host_session_id],
        )?;
        let mut surfaces = Vec::new();
        for task in tasks {
            let Some(window_id) = task.current_pending_window_ref.clone() else {
                continue;
            };
            let Some(window) = self.load_pending_decision_window(&window_id)? else {
                return Err(anyhow!(
                    "control surface query conflict: managed_task_ref={} points to missing pending decision window {}",
                    task.managed_task_ref,
                    window_id,
                ));
            };
            if window.status != PendingDecisionWindowStatus::Open {
                return Err(anyhow!(
                    "control surface query conflict: managed_task_ref={} points to non-open pending decision window {} with status {:?}",
                    task.managed_task_ref,
                    window_id,
                    window.status,
                ));
            }
            surfaces.push(CurrentControlSurfaceProjection {
                host_session_id: task.host_session_id.clone(),
                surface_type: map_surface_type(window.kind),
                managed_task_ref: window.managed_task_ref,
                decision_token: window.decision_token,
                allowed_actions: window.allowed_actions,
            });
        }

        match surfaces.len() {
            0 => Ok(None),
            1 => Ok(surfaces.into_iter().next()),
            _ => Err(anyhow!(
                "control surface query conflict: host_session_id={} has {} open windows",
                host_session_id,
                surfaces.len(),
            )),
        }
    }
}

fn map_surface_type(kind: PendingDecisionWindowKind) -> ControlSurfaceType {
    match kind {
        PendingDecisionWindowKind::StartCandidate => ControlSurfaceType::StartCard,
        PendingDecisionWindowKind::BoundaryConfirmation => ControlSurfaceType::BoundaryCard,
        PendingDecisionWindowKind::ApprovalRequest => ControlSurfaceType::ApprovalRequest,
    }
}
