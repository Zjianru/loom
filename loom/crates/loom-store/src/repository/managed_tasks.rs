use crate::{
    LoomStore, LoomStoreTx, RUNTIME_PROJECTIONS_DIR, RUNTIME_TASKS_DIR, load_json_row_from_conn,
};
use anyhow::{Context, Result};
use loom_domain::{ManagedTask, ManagedTaskRef, WorkflowStage};
use rusqlite::params;

impl LoomStore {
    pub fn save_managed_task(&self, task: &ManagedTask) -> Result<()> {
        let payload_json = serde_json::to_string(task).context("serializing managed task")?;
        let conn = self.connection()?;
        conn.execute(
            "
            INSERT INTO managed_tasks (
                managed_task_ref, host_session_id, workflow_stage, managed_task_class, work_horizon,
                current_scope_version, current_pending_window_ref, current_execution_authorization_ref,
                current_baseline_risk_ref, active_run_ref, payload_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ON CONFLICT(managed_task_ref) DO UPDATE SET
                host_session_id=excluded.host_session_id,
                workflow_stage=excluded.workflow_stage,
                managed_task_class=excluded.managed_task_class,
                work_horizon=excluded.work_horizon,
                current_scope_version=excluded.current_scope_version,
                current_pending_window_ref=excluded.current_pending_window_ref,
                current_execution_authorization_ref=excluded.current_execution_authorization_ref,
                current_baseline_risk_ref=excluded.current_baseline_risk_ref,
                active_run_ref=excluded.active_run_ref,
                payload_json=excluded.payload_json
            ",
            params![
                task.managed_task_ref,
                task.host_session_id,
                serde_json::to_string(&task.workflow_stage)?,
                serde_json::to_string(&task.managed_task_class)?,
                serde_json::to_string(&task.work_horizon)?,
                task.current_scope_version,
                task.current_pending_window_ref,
                task.current_execution_authorization_ref,
                task.current_baseline_risk_ref,
                task.active_run_ref,
                payload_json,
            ],
        )
        .context("upserting managed task")?;
        drop(conn);
        self.write_json(
            self.runtime_root()
                .join(RUNTIME_TASKS_DIR)
                .join(format!("{}.json", task.managed_task_ref)),
            task,
        )?;
        self.write_json(
            self.runtime_root()
                .join(RUNTIME_PROJECTIONS_DIR)
                .join("tasks")
                .join(format!("{}.json", task.managed_task_ref)),
            task,
        )?;
        Ok(())
    }

    pub fn load_managed_task(
        &self,
        managed_task_ref: &ManagedTaskRef,
    ) -> Result<Option<ManagedTask>> {
        self.load_json_row(
            "SELECT payload_json FROM managed_tasks WHERE managed_task_ref = ?1",
            params![managed_task_ref],
        )
    }

    pub fn find_active_task_for_session(
        &self,
        host_session_id: &loom_domain::HostSessionId,
    ) -> Result<Option<ManagedTask>> {
        self.load_json_row(
            "
            SELECT payload_json
            FROM managed_tasks
            WHERE host_session_id = ?1 AND workflow_stage = ?2
            ORDER BY rowid DESC
            LIMIT 1
            ",
            params![
                host_session_id,
                serde_json::to_string(&WorkflowStage::Execute)?
            ],
        )
    }
}

impl LoomStoreTx<'_> {
    pub fn save_managed_task(&mut self, task: &ManagedTask) -> Result<()> {
        self.maybe_fail("tx.save_managed_task")?;
        let payload_json = serde_json::to_string(task).context("serializing managed task")?;
        self.conn
            .execute(
                "
                INSERT INTO managed_tasks (
                    managed_task_ref, host_session_id, workflow_stage, managed_task_class, work_horizon,
                    current_scope_version, current_pending_window_ref, current_execution_authorization_ref,
                    current_baseline_risk_ref, active_run_ref, payload_json
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                ON CONFLICT(managed_task_ref) DO UPDATE SET
                    host_session_id=excluded.host_session_id,
                    workflow_stage=excluded.workflow_stage,
                    managed_task_class=excluded.managed_task_class,
                    work_horizon=excluded.work_horizon,
                    current_scope_version=excluded.current_scope_version,
                    current_pending_window_ref=excluded.current_pending_window_ref,
                    current_execution_authorization_ref=excluded.current_execution_authorization_ref,
                    current_baseline_risk_ref=excluded.current_baseline_risk_ref,
                    active_run_ref=excluded.active_run_ref,
                    payload_json=excluded.payload_json
                ",
                params![
                    task.managed_task_ref,
                    task.host_session_id,
                    serde_json::to_string(&task.workflow_stage)?,
                    serde_json::to_string(&task.managed_task_class)?,
                    serde_json::to_string(&task.work_horizon)?,
                    task.current_scope_version,
                    task.current_pending_window_ref,
                    task.current_execution_authorization_ref,
                    task.current_baseline_risk_ref,
                    task.active_run_ref,
                    payload_json,
                ],
            )
            .context("upserting managed task")?;
        self.stage_json(
            self.runtime_root()
                .join(RUNTIME_TASKS_DIR)
                .join(format!("{}.json", task.managed_task_ref)),
            task,
        )?;
        self.stage_json(
            self.runtime_root()
                .join(RUNTIME_PROJECTIONS_DIR)
                .join("tasks")
                .join(format!("{}.json", task.managed_task_ref)),
            task,
        )?;
        Ok(())
    }

    pub fn load_managed_task(
        &self,
        managed_task_ref: &ManagedTaskRef,
    ) -> Result<Option<ManagedTask>> {
        load_json_row_from_conn(
            &self.conn,
            "SELECT payload_json FROM managed_tasks WHERE managed_task_ref = ?1",
            params![managed_task_ref],
        )
    }

    pub fn find_active_task_for_session(
        &self,
        host_session_id: &loom_domain::HostSessionId,
    ) -> Result<Option<ManagedTask>> {
        load_json_row_from_conn(
            &self.conn,
            "
            SELECT payload_json
            FROM managed_tasks
            WHERE host_session_id = ?1 AND workflow_stage = ?2
            ORDER BY rowid DESC
            LIMIT 1
            ",
            params![
                host_session_id,
                serde_json::to_string(&WorkflowStage::Execute)?
            ],
        )
    }
}
