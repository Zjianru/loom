use crate::{LoomStore, LoomStoreTx, load_json_row_from_conn};
use anyhow::{Context, Result, anyhow};
use loom_domain::{PendingDecisionWindow, PendingDecisionWindowId, PendingDecisionWindowStatus};
use rusqlite::params;

impl LoomStore {
    pub fn save_pending_decision_window(&self, window: &PendingDecisionWindow) -> Result<()> {
        let payload_json =
            serde_json::to_string(window).context("serializing pending decision window")?;
        let conn = self.connection()?;
        conn.execute(
            "
            INSERT INTO pending_decision_windows (
                window_id, managed_task_ref, decision_token, kind, status, payload_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(window_id) DO UPDATE SET status = excluded.status, payload_json = excluded.payload_json
            ",
            params![
                window.window_id,
                window.managed_task_ref,
                window.decision_token,
                serde_json::to_string(&window.kind)?,
                serde_json::to_string(&window.status)?,
                payload_json,
            ],
        )
        .context("upserting pending decision window")?;
        Ok(())
    }

    pub fn update_window_status(
        &self,
        window_id: &PendingDecisionWindowId,
        status: PendingDecisionWindowStatus,
    ) -> Result<()> {
        let mut window = self
            .load_pending_decision_window(window_id)?
            .ok_or_else(|| anyhow!("pending decision window not found: {window_id}"))?;
        window.status = status;
        self.save_pending_decision_window(&window)
    }

    pub fn load_pending_decision_window(
        &self,
        window_id: &PendingDecisionWindowId,
    ) -> Result<Option<PendingDecisionWindow>> {
        self.load_json_row(
            "SELECT payload_json FROM pending_decision_windows WHERE window_id = ?1",
            params![window_id],
        )
    }

    pub fn find_open_window_by_token(
        &self,
        decision_token: &str,
    ) -> Result<Option<PendingDecisionWindow>> {
        self.load_json_row(
            "
            SELECT payload_json
            FROM pending_decision_windows
            WHERE decision_token = ?1 AND status = ?2
            LIMIT 1
            ",
            params![
                decision_token,
                serde_json::to_string(&PendingDecisionWindowStatus::Open)?
            ],
        )
    }
}

impl LoomStoreTx<'_> {
    pub fn save_pending_decision_window(&mut self, window: &PendingDecisionWindow) -> Result<()> {
        self.maybe_fail("tx.save_pending_decision_window")?;
        let payload_json =
            serde_json::to_string(window).context("serializing pending decision window")?;
        self.conn
            .execute(
                "
                INSERT INTO pending_decision_windows (
                    window_id, managed_task_ref, decision_token, kind, status, payload_json
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                ON CONFLICT(window_id) DO UPDATE SET status = excluded.status, payload_json = excluded.payload_json
                ",
                params![
                    window.window_id,
                    window.managed_task_ref,
                    window.decision_token,
                    serde_json::to_string(&window.kind)?,
                    serde_json::to_string(&window.status)?,
                    payload_json,
                ],
            )
            .context("upserting pending decision window")?;
        Ok(())
    }

    pub fn update_window_status(
        &mut self,
        window_id: &PendingDecisionWindowId,
        status: PendingDecisionWindowStatus,
    ) -> Result<()> {
        let mut window = self
            .load_pending_decision_window(window_id)?
            .ok_or_else(|| anyhow!("pending decision window not found: {window_id}"))?;
        window.status = status;
        self.save_pending_decision_window(&window)
    }

    pub fn load_pending_decision_window(
        &self,
        window_id: &PendingDecisionWindowId,
    ) -> Result<Option<PendingDecisionWindow>> {
        load_json_row_from_conn(
            &self.conn,
            "SELECT payload_json FROM pending_decision_windows WHERE window_id = ?1",
            params![window_id],
        )
    }

    pub fn find_open_window_by_token(
        &self,
        decision_token: &str,
    ) -> Result<Option<PendingDecisionWindow>> {
        load_json_row_from_conn(
            &self.conn,
            "
            SELECT payload_json
            FROM pending_decision_windows
            WHERE decision_token = ?1 AND status = ?2
            LIMIT 1
            ",
            params![
                decision_token,
                serde_json::to_string(&PendingDecisionWindowStatus::Open)?
            ],
        )
    }
}
