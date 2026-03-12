use crate::{
    LoomStore, LoomStoreTx, RUNTIME_TASKS_DIR, load_json_row_from_conn, load_json_rows_from_conn,
};
use anyhow::{Context, Result};
use loom_domain::{ManagedTaskRef, TaskScopeSnapshot};
use rusqlite::params;

impl LoomStore {
    pub fn save_scope_snapshot(&self, scope: &TaskScopeSnapshot) -> Result<()> {
        let payload_json =
            serde_json::to_string(scope).context("serializing task scope snapshot")?;
        let conn = self.connection()?;
        conn.execute(
            "
            INSERT INTO task_scope_snapshots (scope_id, managed_task_ref, scope_version, payload_json)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(scope_id) DO UPDATE SET payload_json = excluded.payload_json
            ",
            params![scope.scope_id, scope.managed_task_ref, scope.scope_version, payload_json],
        )
        .context("upserting task scope snapshot")?;
        drop(conn);
        self.write_json(
            self.runtime_root().join(RUNTIME_TASKS_DIR).join(format!(
                "{}-scope-v{}.json",
                scope.managed_task_ref, scope.scope_version
            )),
            scope,
        )?;
        Ok(())
    }

    pub fn list_scope_snapshots(
        &self,
        managed_task_ref: &ManagedTaskRef,
    ) -> Result<Vec<TaskScopeSnapshot>> {
        self.load_json_rows(
            "
            SELECT payload_json
            FROM task_scope_snapshots
            WHERE managed_task_ref = ?1
            ORDER BY scope_version ASC
            ",
            params![managed_task_ref],
        )
    }

    pub fn latest_scope_snapshot(
        &self,
        managed_task_ref: &ManagedTaskRef,
    ) -> Result<Option<TaskScopeSnapshot>> {
        self.load_json_row(
            "
            SELECT payload_json
            FROM task_scope_snapshots
            WHERE managed_task_ref = ?1
            ORDER BY scope_version DESC
            LIMIT 1
            ",
            params![managed_task_ref],
        )
    }
}

impl LoomStoreTx<'_> {
    pub fn save_scope_snapshot(&mut self, scope: &TaskScopeSnapshot) -> Result<()> {
        self.maybe_fail("tx.save_scope_snapshot")?;
        let payload_json =
            serde_json::to_string(scope).context("serializing task scope snapshot")?;
        self.conn
            .execute(
                "
                INSERT INTO task_scope_snapshots (scope_id, managed_task_ref, scope_version, payload_json)
                VALUES (?1, ?2, ?3, ?4)
                ON CONFLICT(scope_id) DO UPDATE SET payload_json = excluded.payload_json
                ",
                params![scope.scope_id, scope.managed_task_ref, scope.scope_version, payload_json],
            )
            .context("upserting task scope snapshot")?;
        self.stage_json(
            self.runtime_root().join(RUNTIME_TASKS_DIR).join(format!(
                "{}-scope-v{}.json",
                scope.managed_task_ref, scope.scope_version
            )),
            scope,
        )?;
        Ok(())
    }

    pub fn list_scope_snapshots(
        &self,
        managed_task_ref: &ManagedTaskRef,
    ) -> Result<Vec<TaskScopeSnapshot>> {
        load_json_rows_from_conn(
            &self.conn,
            "
            SELECT payload_json
            FROM task_scope_snapshots
            WHERE managed_task_ref = ?1
            ORDER BY scope_version ASC
            ",
            params![managed_task_ref],
        )
    }

    pub fn latest_scope_snapshot(
        &self,
        managed_task_ref: &ManagedTaskRef,
    ) -> Result<Option<TaskScopeSnapshot>> {
        load_json_row_from_conn(
            &self.conn,
            "
            SELECT payload_json
            FROM task_scope_snapshots
            WHERE managed_task_ref = ?1
            ORDER BY scope_version DESC
            LIMIT 1
            ",
            params![managed_task_ref],
        )
    }
}
