use crate::{LoomStore, LoomStoreTx, load_json_row_from_conn, load_json_rows_from_conn};
use anyhow::{Context, Result, anyhow};
use loom_domain::{
    ExecutionAuthorization, ExecutionAuthorizationId, ExecutionAuthorizationStatus, ManagedTaskRef,
};
use rusqlite::params;

impl LoomStore {
    pub fn save_execution_authorization(
        &self,
        authorization: &ExecutionAuthorization,
    ) -> Result<()> {
        let payload_json =
            serde_json::to_string(authorization).context("serializing execution authorization")?;
        let conn = self.connection()?;
        conn.execute(
            "
            INSERT INTO execution_authorizations (
                authorization_id, managed_task_ref, capability_snapshot_ref, task_scope_ref, status, supersedes, payload_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(authorization_id) DO UPDATE SET payload_json = excluded.payload_json, status = excluded.status
            ",
            params![
                authorization.authorization_id,
                authorization.managed_task_ref,
                authorization.capability_snapshot_ref,
                authorization.task_scope_ref,
                serde_json::to_string(&authorization.status)?,
                authorization.supersedes,
                payload_json,
            ],
        )
        .context("upserting execution authorization")?;
        Ok(())
    }

    pub fn update_authorization_status(
        &self,
        authorization_id: &ExecutionAuthorizationId,
        status: ExecutionAuthorizationStatus,
    ) -> Result<()> {
        let mut authorization = self
            .load_execution_authorization(authorization_id)?
            .ok_or_else(|| anyhow!("authorization not found: {authorization_id}"))?;
        authorization.status = status;
        self.save_execution_authorization(&authorization)
    }

    pub fn load_execution_authorization(
        &self,
        authorization_id: &ExecutionAuthorizationId,
    ) -> Result<Option<ExecutionAuthorization>> {
        self.load_json_row(
            "SELECT payload_json FROM execution_authorizations WHERE authorization_id = ?1",
            params![authorization_id],
        )
    }

    pub fn latest_execution_authorization(
        &self,
        managed_task_ref: &ManagedTaskRef,
    ) -> Result<Option<ExecutionAuthorization>> {
        self.load_json_row(
            "
            SELECT payload_json
            FROM execution_authorizations
            WHERE managed_task_ref = ?1
            ORDER BY rowid DESC
            LIMIT 1
            ",
            params![managed_task_ref],
        )
    }

    pub fn list_execution_authorizations(
        &self,
        managed_task_ref: &ManagedTaskRef,
    ) -> Result<Vec<ExecutionAuthorization>> {
        self.load_json_rows(
            "
            SELECT payload_json
            FROM execution_authorizations
            WHERE managed_task_ref = ?1
            ORDER BY rowid ASC
            ",
            params![managed_task_ref],
        )
    }
}

impl LoomStoreTx<'_> {
    pub fn save_execution_authorization(
        &mut self,
        authorization: &ExecutionAuthorization,
    ) -> Result<()> {
        self.maybe_fail("tx.save_execution_authorization")?;
        let payload_json =
            serde_json::to_string(authorization).context("serializing execution authorization")?;
        self.conn
            .execute(
                "
                INSERT INTO execution_authorizations (
                    authorization_id, managed_task_ref, capability_snapshot_ref, task_scope_ref, status, supersedes, payload_json
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                ON CONFLICT(authorization_id) DO UPDATE SET payload_json = excluded.payload_json, status = excluded.status
                ",
                params![
                    authorization.authorization_id,
                    authorization.managed_task_ref,
                    authorization.capability_snapshot_ref,
                    authorization.task_scope_ref,
                    serde_json::to_string(&authorization.status)?,
                    authorization.supersedes,
                    payload_json,
                ],
            )
            .context("upserting execution authorization")?;
        Ok(())
    }

    pub fn update_authorization_status(
        &mut self,
        authorization_id: &ExecutionAuthorizationId,
        status: ExecutionAuthorizationStatus,
    ) -> Result<()> {
        let mut authorization = self
            .load_execution_authorization(authorization_id)?
            .ok_or_else(|| anyhow!("authorization not found: {authorization_id}"))?;
        authorization.status = status;
        self.save_execution_authorization(&authorization)
    }

    pub fn load_execution_authorization(
        &self,
        authorization_id: &ExecutionAuthorizationId,
    ) -> Result<Option<ExecutionAuthorization>> {
        load_json_row_from_conn(
            &self.conn,
            "SELECT payload_json FROM execution_authorizations WHERE authorization_id = ?1",
            params![authorization_id],
        )
    }

    pub fn list_execution_authorizations(
        &self,
        managed_task_ref: &ManagedTaskRef,
    ) -> Result<Vec<ExecutionAuthorization>> {
        load_json_rows_from_conn(
            &self.conn,
            "
            SELECT payload_json
            FROM execution_authorizations
            WHERE managed_task_ref = ?1
            ORDER BY rowid ASC
            ",
            params![managed_task_ref],
        )
    }
}
