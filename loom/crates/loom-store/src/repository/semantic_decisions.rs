use crate::{LoomStore, LoomStoreTx, load_json_row_from_conn, load_json_rows_from_conn};
use anyhow::{Context, Result};
use loom_domain::{ControlActionEnvelope, SemanticDecisionBatchEnvelope, SemanticDecisionEnvelope};
use rusqlite::{OptionalExtension, params};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistOutcome {
    Inserted,
    DuplicateSame,
    Conflict,
}

impl LoomStore {
    pub fn semantic_decision_persist_outcome(
        &self,
        decision: &SemanticDecisionEnvelope,
    ) -> Result<PersistOutcome> {
        let payload_json =
            serde_json::to_string(decision).context("serializing semantic decision")?;
        let conn = self.connection()?;
        let existing: Option<String> = conn
            .query_row(
                "
                SELECT payload_json
                FROM semantic_decisions
                WHERE decision_ref = ?1
                ",
                params![decision.decision_ref],
                |row| row.get(0),
            )
            .optional()
            .context("loading semantic decision for idempotency")?;
        Ok(match existing {
            Some(existing_payload) if existing_payload == payload_json => {
                PersistOutcome::DuplicateSame
            }
            Some(_) => PersistOutcome::Conflict,
            None => PersistOutcome::Inserted,
        })
    }

    pub fn save_semantic_decision_batch(
        &self,
        batch: &SemanticDecisionBatchEnvelope,
        status: &str,
        rejection_reason: Option<&str>,
    ) -> Result<()> {
        let payload_json =
            serde_json::to_string(batch).context("serializing semantic decision batch")?;
        let conn = self.connection()?;
        conn.execute(
            "
            INSERT INTO semantic_decision_batches (
                batch_ref, host_session_id, status, issued_at, rejection_reason, payload_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(batch_ref) DO UPDATE SET
                host_session_id = excluded.host_session_id,
                status = excluded.status,
                issued_at = excluded.issued_at,
                rejection_reason = excluded.rejection_reason,
                payload_json = excluded.payload_json
            ",
            params![
                batch.meta.ingress_id,
                batch.host_session_id,
                status,
                batch.issued_at,
                rejection_reason,
                payload_json,
            ],
        )
        .context("saving semantic decision batch")?;
        Ok(())
    }

    pub fn semantic_decision_batch_status(&self, batch_ref: &str) -> Result<Option<String>> {
        let conn = self.connection()?;
        conn.query_row(
            "
            SELECT status
            FROM semantic_decision_batches
            WHERE batch_ref = ?1
            ",
            params![batch_ref],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .context("loading semantic decision batch status")
    }

    pub fn save_semantic_decision(
        &self,
        batch_ref: &str,
        decision: &SemanticDecisionEnvelope,
    ) -> Result<PersistOutcome> {
        let payload_json =
            serde_json::to_string(decision).context("serializing semantic decision")?;
        let persist_outcome = self.semantic_decision_persist_outcome(decision)?;
        if persist_outcome != PersistOutcome::Inserted {
            return Ok(persist_outcome);
        }
        let conn = self.connection()?;
        conn.execute(
            "
            INSERT INTO semantic_decisions (
                decision_ref, batch_ref, managed_task_ref, decision_kind, issued_at, payload_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ",
            params![
                decision.decision_ref,
                batch_ref,
                decision.managed_task_ref,
                serde_json::to_string(&decision.decision_kind)
                    .context("serializing semantic decision kind")?,
                decision.issued_at,
                payload_json,
            ],
        )
        .context("saving semantic decision")?;
        Ok(PersistOutcome::Inserted)
    }

    pub fn load_semantic_decision(
        &self,
        decision_ref: &str,
    ) -> Result<Option<SemanticDecisionEnvelope>> {
        self.load_json_row(
            "
            SELECT payload_json
            FROM semantic_decisions
            WHERE decision_ref = ?1
            ",
            params![decision_ref],
        )
    }

    pub fn list_semantic_decisions_for_batch(
        &self,
        batch_ref: &str,
    ) -> Result<Vec<SemanticDecisionEnvelope>> {
        self.load_json_rows(
            "
            SELECT payload_json
            FROM semantic_decisions
            WHERE batch_ref = ?1
            ORDER BY rowid ASC
            ",
            params![batch_ref],
        )
    }

    pub fn save_control_action_envelope(
        &self,
        envelope: &ControlActionEnvelope,
    ) -> Result<PersistOutcome> {
        let persist_outcome = self.control_action_envelope_persist_outcome(envelope)?;
        if persist_outcome != PersistOutcome::Inserted {
            return Ok(persist_outcome);
        }
        let payload_json =
            serde_json::to_string(envelope).context("serializing control action envelope")?;
        let conn = self.connection()?;
        conn.execute(
            "
            INSERT INTO control_action_envelopes (
                decision_ref, managed_task_ref, action_kind, issued_at, payload_json
            ) VALUES (?1, ?2, ?3, ?4, ?5)
            ",
            params![
                envelope.decision_ref,
                envelope.action.managed_task_ref,
                serde_json::to_string(&envelope.action.kind)
                    .context("serializing control action kind")?,
                envelope.issued_at,
                payload_json,
            ],
        )
        .context("saving control action envelope")?;
        Ok(PersistOutcome::Inserted)
    }

    pub fn control_action_envelope_persist_outcome(
        &self,
        envelope: &ControlActionEnvelope,
    ) -> Result<PersistOutcome> {
        let payload_json =
            serde_json::to_string(envelope).context("serializing control action envelope")?;
        let conn = self.connection()?;
        let existing: Option<String> = conn
            .query_row(
                "
                SELECT payload_json
                FROM control_action_envelopes
                WHERE decision_ref = ?1
                ",
                params![envelope.decision_ref],
                |row| row.get(0),
            )
            .optional()
            .context("loading control action envelope for idempotency")?;
        Ok(match existing {
            Some(existing_payload) if existing_payload == payload_json => {
                PersistOutcome::DuplicateSame
            }
            Some(_) => PersistOutcome::Conflict,
            None => PersistOutcome::Inserted,
        })
    }
}

impl LoomStoreTx<'_> {
    pub fn semantic_decision_persist_outcome(
        &self,
        decision: &SemanticDecisionEnvelope,
    ) -> Result<PersistOutcome> {
        let payload_json =
            serde_json::to_string(decision).context("serializing semantic decision")?;
        let existing: Option<String> = self
            .conn
            .query_row(
                "
                SELECT payload_json
                FROM semantic_decisions
                WHERE decision_ref = ?1
                ",
                params![decision.decision_ref],
                |row| row.get(0),
            )
            .optional()
            .context("loading semantic decision for idempotency")?;
        Ok(match existing {
            Some(existing_payload) if existing_payload == payload_json => {
                PersistOutcome::DuplicateSame
            }
            Some(_) => PersistOutcome::Conflict,
            None => PersistOutcome::Inserted,
        })
    }

    pub fn save_semantic_decision_batch(
        &mut self,
        batch: &SemanticDecisionBatchEnvelope,
        status: &str,
        rejection_reason: Option<&str>,
    ) -> Result<()> {
        self.maybe_fail("tx.save_semantic_decision_batch")?;
        let payload_json =
            serde_json::to_string(batch).context("serializing semantic decision batch")?;
        self.conn
            .execute(
                "
                INSERT INTO semantic_decision_batches (
                    batch_ref, host_session_id, status, issued_at, rejection_reason, payload_json
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                ON CONFLICT(batch_ref) DO UPDATE SET
                    host_session_id = excluded.host_session_id,
                    status = excluded.status,
                    issued_at = excluded.issued_at,
                    rejection_reason = excluded.rejection_reason,
                    payload_json = excluded.payload_json
                ",
                params![
                    batch.meta.ingress_id,
                    batch.host_session_id,
                    status,
                    batch.issued_at,
                    rejection_reason,
                    payload_json,
                ],
            )
            .context("saving semantic decision batch")?;
        Ok(())
    }

    pub fn semantic_decision_batch_status(&self, batch_ref: &str) -> Result<Option<String>> {
        self.conn
            .query_row(
                "
                SELECT status
                FROM semantic_decision_batches
                WHERE batch_ref = ?1
                ",
                params![batch_ref],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .context("loading semantic decision batch status")
    }

    pub fn save_semantic_decision(
        &mut self,
        batch_ref: &str,
        decision: &SemanticDecisionEnvelope,
    ) -> Result<PersistOutcome> {
        self.maybe_fail("tx.save_semantic_decision")?;
        let payload_json =
            serde_json::to_string(decision).context("serializing semantic decision")?;
        let persist_outcome = self.semantic_decision_persist_outcome(decision)?;
        if persist_outcome != PersistOutcome::Inserted {
            return Ok(persist_outcome);
        }
        self.conn
            .execute(
                "
                INSERT INTO semantic_decisions (
                    decision_ref, batch_ref, managed_task_ref, decision_kind, issued_at, payload_json
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                ",
                params![
                    decision.decision_ref,
                    batch_ref,
                    decision.managed_task_ref,
                    serde_json::to_string(&decision.decision_kind)
                        .context("serializing semantic decision kind")?,
                    decision.issued_at,
                    payload_json,
                ],
            )
            .context("saving semantic decision")?;
        Ok(PersistOutcome::Inserted)
    }

    pub fn load_semantic_decision(
        &self,
        decision_ref: &str,
    ) -> Result<Option<SemanticDecisionEnvelope>> {
        load_json_row_from_conn(
            &self.conn,
            "
            SELECT payload_json
            FROM semantic_decisions
            WHERE decision_ref = ?1
            ",
            params![decision_ref],
        )
    }

    pub fn list_semantic_decisions_for_batch(
        &self,
        batch_ref: &str,
    ) -> Result<Vec<SemanticDecisionEnvelope>> {
        load_json_rows_from_conn(
            &self.conn,
            "
            SELECT payload_json
            FROM semantic_decisions
            WHERE batch_ref = ?1
            ORDER BY rowid ASC
            ",
            params![batch_ref],
        )
    }

    pub fn control_action_envelope_persist_outcome(
        &self,
        envelope: &ControlActionEnvelope,
    ) -> Result<PersistOutcome> {
        let payload_json =
            serde_json::to_string(envelope).context("serializing control action envelope")?;
        let existing: Option<String> = self
            .conn
            .query_row(
                "
                SELECT payload_json
                FROM control_action_envelopes
                WHERE decision_ref = ?1
                ",
                params![envelope.decision_ref],
                |row| row.get(0),
            )
            .optional()
            .context("loading control action envelope for idempotency")?;
        Ok(match existing {
            Some(existing_payload) if existing_payload == payload_json => {
                PersistOutcome::DuplicateSame
            }
            Some(_) => PersistOutcome::Conflict,
            None => PersistOutcome::Inserted,
        })
    }

    pub fn save_control_action_envelope(
        &mut self,
        envelope: &ControlActionEnvelope,
    ) -> Result<PersistOutcome> {
        self.maybe_fail("tx.save_control_action_envelope")?;
        let persist_outcome = self.control_action_envelope_persist_outcome(envelope)?;
        if persist_outcome != PersistOutcome::Inserted {
            return Ok(persist_outcome);
        }
        let payload_json =
            serde_json::to_string(envelope).context("serializing control action envelope")?;
        self.conn
            .execute(
                "
                INSERT INTO control_action_envelopes (
                    decision_ref, managed_task_ref, action_kind, issued_at, payload_json
                ) VALUES (?1, ?2, ?3, ?4, ?5)
                ",
                params![
                    envelope.decision_ref,
                    envelope.action.managed_task_ref,
                    serde_json::to_string(&envelope.action.kind)
                        .context("serializing control action kind")?,
                    envelope.issued_at,
                    payload_json,
                ],
            )
            .context("saving control action envelope")?;
        Ok(PersistOutcome::Inserted)
    }
}
