use crate::{LoomStore, RUNTIME_BRIDGES_DIR};
use anyhow::{Context, Result};
use loom_domain::{
    DeliveryStatus, HostSessionId, KernelOutboundPayload, OutboundDelivery, OutboundDeliveryId,
    new_id, now_timestamp,
};
use rusqlite::params;

const DEFAULT_OUTBOUND_MAX_ATTEMPTS: u32 = 6;

fn managed_task_ref_from_payload(
    payload: &KernelOutboundPayload,
) -> Option<loom_domain::ManagedTaskRef> {
    match payload {
        KernelOutboundPayload::StartCard(value) => Some(value.managed_task_ref.clone()),
        KernelOutboundPayload::BoundaryCard(value) => Some(value.managed_task_ref.clone()),
        KernelOutboundPayload::ResultSummary(value) => Some(value.managed_task_ref.clone()),
        KernelOutboundPayload::ApprovalRequest(value) => Some(value.managed_task_ref.clone()),
        KernelOutboundPayload::SuppressHostMessage(_) => None,
        KernelOutboundPayload::ToolDecision(value) => Some(value.managed_task_ref.clone()),
    }
}

impl LoomStore {
    pub fn enqueue_outbound(
        &self,
        host_session_id: HostSessionId,
        payload: KernelOutboundPayload,
    ) -> Result<OutboundDelivery> {
        let delivery = OutboundDelivery {
            delivery_id: loom_domain::new_id("delivery"),
            host_session_id,
            managed_task_ref: managed_task_ref_from_payload(&payload),
            correlation_id: new_id("corr"),
            causation_id: None,
            payload,
            delivery_status: DeliveryStatus::Pending,
            attempts: 0,
            max_attempts: DEFAULT_OUTBOUND_MAX_ATTEMPTS,
            next_attempt_at: None,
            expires_at: None,
            last_error: None,
            created_at: now_timestamp(),
            acked_at: None,
        };
        let payload_json =
            serde_json::to_string(&delivery).context("serializing outbound delivery")?;
        let conn = self.connection()?;
        conn.execute(
            "
            INSERT INTO outbound_deliveries (
                delivery_id, host_session_id, managed_task_ref, correlation_id, causation_id,
                delivery_status, payload_json, attempts, max_attempts, next_attempt_at, expires_at,
                last_error, created_at, acked_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
            ",
            params![
                delivery.delivery_id,
                delivery.host_session_id,
                delivery.managed_task_ref,
                delivery.correlation_id,
                delivery.causation_id,
                serde_json::to_string(&delivery.delivery_status)?,
                payload_json,
                delivery.attempts,
                delivery.max_attempts,
                delivery.next_attempt_at,
                delivery.expires_at,
                delivery.last_error,
                delivery.created_at,
                delivery.acked_at
            ],
        )
        .context("enqueueing outbound delivery")?;
        drop(conn);
        self.write_json(
            self.runtime_root()
                .join(RUNTIME_BRIDGES_DIR)
                .join("outbound")
                .join(format!("{}.json", delivery.delivery_id)),
            &delivery,
        )?;
        Ok(delivery)
    }

    pub fn next_outbound(
        &self,
        host_session_id: &HostSessionId,
    ) -> Result<Option<OutboundDelivery>> {
        let now = now_timestamp();
        let Some(mut delivery): Option<OutboundDelivery> = self.load_json_row(
            "
            SELECT payload_json
            FROM outbound_deliveries
            WHERE host_session_id = ?1
              AND (
                delivery_status = ?2
                OR (
                    delivery_status = ?3
                    AND (next_attempt_at IS NULL OR CAST(next_attempt_at AS INTEGER) <= CAST(?4 AS INTEGER))
                )
              )
            ORDER BY sequence_id ASC
            LIMIT 1
            ",
            params![
                host_session_id,
                serde_json::to_string(&DeliveryStatus::Pending)?,
                serde_json::to_string(&DeliveryStatus::RetryScheduled)?,
                now,
            ],
        )? else {
            return Ok(None);
        };
        delivery.delivery_status = DeliveryStatus::Delivering;
        delivery.attempts += 1;
        delivery.next_attempt_at = None;
        let payload_json =
            serde_json::to_string(&delivery).context("serializing delivering outbound")?;
        let conn = self.connection()?;
        conn.execute(
            "
            UPDATE outbound_deliveries
            SET delivery_status = ?2, attempts = ?3, next_attempt_at = ?4, payload_json = ?5
            WHERE delivery_id = ?1
            ",
            params![
                delivery.delivery_id,
                serde_json::to_string(&delivery.delivery_status)?,
                delivery.attempts,
                delivery.next_attempt_at,
                payload_json,
            ],
        )
        .context("marking outbound delivering")?;
        drop(conn);
        self.write_json(
            self.runtime_root()
                .join(RUNTIME_BRIDGES_DIR)
                .join("outbound")
                .join(format!("{}.json", delivery.delivery_id)),
            &delivery,
        )?;
        Ok(Some(delivery))
    }

    pub fn ack_outbound(&self, delivery_id: &OutboundDeliveryId) -> Result<bool> {
        let Some(mut delivery) = self.load_outbound(delivery_id)? else {
            return Ok(false);
        };
        if delivery.delivery_status == DeliveryStatus::Acked {
            return Ok(true);
        }
        if matches!(
            delivery.delivery_status,
            DeliveryStatus::Expired | DeliveryStatus::TerminalFailed
        ) {
            return Ok(false);
        }
        delivery.delivery_status = DeliveryStatus::Acked;
        delivery.acked_at = Some(now_timestamp());
        let payload_json =
            serde_json::to_string(&delivery).context("serializing acknowledged delivery")?;
        let conn = self.connection()?;
        conn.execute(
            "
            UPDATE outbound_deliveries
            SET delivery_status = ?2, payload_json = ?3, acked_at = ?4
            WHERE delivery_id = ?1
            ",
            params![
                delivery_id,
                serde_json::to_string(&delivery.delivery_status)?,
                payload_json,
                delivery.acked_at
            ],
        )
        .context("acknowledging outbound delivery")?;
        drop(conn);
        self.write_json(
            self.runtime_root()
                .join(RUNTIME_BRIDGES_DIR)
                .join("outbound")
                .join(format!("{}.json", delivery.delivery_id)),
            &delivery,
        )?;
        Ok(true)
    }

    pub fn schedule_outbound_retry(
        &self,
        delivery_id: &OutboundDeliveryId,
        next_attempt_at: String,
        last_error: String,
    ) -> Result<bool> {
        let Some(mut delivery) = self.load_outbound(delivery_id)? else {
            return Ok(false);
        };
        if delivery.attempts >= delivery.max_attempts {
            delivery.delivery_status = DeliveryStatus::TerminalFailed;
            delivery.next_attempt_at = None;
            delivery.last_error = Some(last_error);
        } else {
            delivery.delivery_status = DeliveryStatus::RetryScheduled;
            delivery.next_attempt_at = Some(next_attempt_at);
            delivery.last_error = Some(last_error);
        }
        let payload_json = serde_json::to_string(&delivery)
            .context("serializing retry scheduled outbound delivery")?;
        let conn = self.connection()?;
        conn.execute(
            "
            UPDATE outbound_deliveries
            SET delivery_status = ?2, next_attempt_at = ?3, last_error = ?4, payload_json = ?5
            WHERE delivery_id = ?1
            ",
            params![
                delivery_id,
                serde_json::to_string(&delivery.delivery_status)?,
                delivery.next_attempt_at,
                delivery.last_error,
                payload_json,
            ],
        )
        .context("scheduling outbound retry")?;
        drop(conn);
        self.write_json(
            self.runtime_root()
                .join(RUNTIME_BRIDGES_DIR)
                .join("outbound")
                .join(format!("{}.json", delivery.delivery_id)),
            &delivery,
        )?;
        Ok(true)
    }

    pub fn load_outbound(
        &self,
        delivery_id: &OutboundDeliveryId,
    ) -> Result<Option<OutboundDelivery>> {
        self.load_json_row(
            "SELECT payload_json FROM outbound_deliveries WHERE delivery_id = ?1",
            params![delivery_id],
        )
    }
}
