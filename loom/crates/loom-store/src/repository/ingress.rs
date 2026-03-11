use crate::{LoomStore, RUNTIME_BRIDGES_DIR};
use anyhow::{Context, Result};
use loom_domain::{CurrentTurnEnvelope, IngressMeta};
use rusqlite::params;

impl LoomStore {
    pub fn record_ingress_receipt<T: serde::Serialize>(
        &self,
        meta: &IngressMeta,
        ingress_kind: &str,
        payload: &T,
    ) -> Result<bool> {
        let payload_json =
            serde_json::to_string(payload).context("serializing ingress receipt payload")?;
        let conn = self.connection()?;
        let inserted = conn
            .execute(
                "
                INSERT INTO ingress_receipts (
                    ingress_id, dedupe_window, ingress_kind, correlation_id, payload_json, received_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                ON CONFLICT(ingress_id, dedupe_window) DO NOTHING
                ",
                params![
                    meta.ingress_id,
                    meta.dedupe_window,
                    ingress_kind,
                    meta.correlation_id,
                    payload_json,
                    meta.received_at,
                ],
            )
            .context("recording ingress receipt")?;
        Ok(inserted > 0)
    }

    pub fn count_ingress_receipts(&self, ingress_kind: &str) -> Result<u32> {
        let conn = self.connection()?;
        let count = conn
            .query_row(
                "SELECT COUNT(*) FROM ingress_receipts WHERE ingress_kind = ?1",
                params![ingress_kind],
                |row| row.get::<_, u32>(0),
            )
            .context("counting ingress receipts")?;
        Ok(count)
    }

    pub fn save_current_turn(&self, turn: &CurrentTurnEnvelope) -> Result<()> {
        let payload_json = serde_json::to_string(turn).context("serializing current turn")?;
        let conn = self.connection()?;
        conn.execute(
            "
            INSERT INTO current_turns (
                ingress_id, host_session_id, host_message_ref, correlation_id, payload_json, received_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(ingress_id) DO UPDATE SET payload_json = excluded.payload_json
            ",
            params![
                turn.meta.ingress_id,
                turn.host_session_id,
                turn.host_message_ref,
                turn.meta.correlation_id,
                payload_json,
                turn.meta.received_at,
            ],
        )
        .context("saving current turn")?;
        drop(conn);
        self.write_json(
            self.runtime_root()
                .join(RUNTIME_BRIDGES_DIR)
                .join("current-turns")
                .join(format!("{}.json", turn.host_session_id)),
            turn,
        )?;
        Ok(())
    }

    pub fn latest_current_turn(
        &self,
        host_session_id: &loom_domain::HostSessionId,
    ) -> Result<Option<CurrentTurnEnvelope>> {
        self.load_json_row(
            "
            SELECT payload_json
            FROM current_turns
            WHERE host_session_id = ?1
            ORDER BY sequence_id DESC
            LIMIT 1
            ",
            params![host_session_id],
        )
    }
}
