use crate::{LoomStore, RUNTIME_BOOTSTRAP_DIR, RUNTIME_BRIDGES_DIR};
use anyhow::{Context, Result};
use loom_domain::{
    BridgeBootstrapMaterial, BridgeBootstrapTicket, BridgeCredentialStatus, BridgeSecretRef,
    BridgeSessionCredential, new_id, now_timestamp,
};
use rusqlite::params;
use sha2::{Digest, Sha256};

fn hash_secret(secret: &str) -> String {
    format!("{:x}", Sha256::digest(secret.as_bytes()))
}

impl LoomStore {
    pub fn hash_bridge_secret(&self, secret: &str) -> String {
        hash_secret(secret)
    }

    pub fn save_bridge_bootstrap_ticket(
        &self,
        ticket: &BridgeBootstrapTicket,
        material: &BridgeBootstrapMaterial,
    ) -> Result<()> {
        let payload_json =
            serde_json::to_string(ticket).context("serializing bridge bootstrap ticket")?;
        let conn = self.connection()?;
        conn.execute(
            "
            INSERT INTO bridge_bootstrap_tickets (
                ticket_id, bridge_instance_id, adapter_id, status, expires_at, payload_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(ticket_id) DO UPDATE SET
                status = excluded.status,
                expires_at = excluded.expires_at,
                payload_json = excluded.payload_json
            ",
            params![
                ticket.ticket_id,
                ticket.bridge_instance_id,
                ticket.adapter_id,
                serde_json::to_string(&ticket.status)?,
                ticket.expires_at,
                payload_json,
            ],
        )
        .context("saving bridge bootstrap ticket")?;
        drop(conn);
        self.write_json(
            self.runtime_root()
                .join(RUNTIME_BOOTSTRAP_DIR)
                .join("bootstrap-ticket.json"),
            material,
        )?;
        self.write_json(
            self.runtime_root()
                .join(RUNTIME_BRIDGES_DIR)
                .join("bridge-auth")
                .join("bootstrap-ticket.json"),
            ticket,
        )?;
        Ok(())
    }

    pub fn load_bridge_bootstrap_ticket(
        &self,
        ticket_id: &loom_domain::BridgeBootstrapTicketId,
    ) -> Result<Option<BridgeBootstrapTicket>> {
        self.load_json_row(
            "SELECT payload_json FROM bridge_bootstrap_tickets WHERE ticket_id = ?1",
            params![ticket_id],
        )
    }

    pub fn consume_bridge_bootstrap_ticket(
        &self,
        ticket_id: &loom_domain::BridgeBootstrapTicketId,
    ) -> Result<()> {
        let Some(mut ticket) = self.load_bridge_bootstrap_ticket(ticket_id)? else {
            return Ok(());
        };
        ticket.status = BridgeCredentialStatus::Consumed;
        let payload_json =
            serde_json::to_string(&ticket).context("serializing consumed bootstrap ticket")?;
        let conn = self.connection()?;
        conn.execute(
            "
            UPDATE bridge_bootstrap_tickets
            SET status = ?2, payload_json = ?3
            WHERE ticket_id = ?1
            ",
            params![
                ticket_id,
                serde_json::to_string(&ticket.status)?,
                payload_json,
            ],
        )
        .context("consuming bootstrap ticket")?;
        drop(conn);
        self.write_json(
            self.runtime_root()
                .join(RUNTIME_BRIDGES_DIR)
                .join("bridge-auth")
                .join("bootstrap-ticket.json"),
            &ticket,
        )?;
        Ok(())
    }

    pub fn save_bridge_session_credential(
        &self,
        credential: &BridgeSessionCredential,
    ) -> Result<()> {
        let payload_json =
            serde_json::to_string(credential).context("serializing bridge session credential")?;
        let conn = self.connection()?;
        conn.execute(
            "
            INSERT INTO bridge_session_credentials (
                credential_id, bridge_instance_id, adapter_id, secret_ref, rotation_epoch, status, payload_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(credential_id) DO UPDATE SET
                status = excluded.status,
                payload_json = excluded.payload_json
            ",
            params![
                credential.credential_id,
                credential.bridge_instance_id,
                credential.adapter_id,
                credential.secret_ref,
                credential.rotation_epoch,
                serde_json::to_string(&credential.status)?,
                payload_json,
            ],
        )
        .context("saving bridge session credential")?;
        drop(conn);
        self.write_json(
            self.runtime_root()
                .join(RUNTIME_BRIDGES_DIR)
                .join("bridge-auth")
                .join("session-credential.json"),
            credential,
        )?;
        Ok(())
    }

    pub fn find_bridge_session_credential_by_secret_ref(
        &self,
        secret_ref: &BridgeSecretRef,
    ) -> Result<Option<BridgeSessionCredential>> {
        self.load_json_row(
            "SELECT payload_json FROM bridge_session_credentials WHERE secret_ref = ?1",
            params![secret_ref],
        )
    }

    pub fn revoke_bridge_credentials_for_instance(&self, bridge_instance_id: &str) -> Result<()> {
        let credentials: Vec<BridgeSessionCredential> = self.load_json_rows(
            "SELECT payload_json FROM bridge_session_credentials WHERE bridge_instance_id = ?1",
            params![bridge_instance_id],
        )?;
        for mut credential in credentials {
            credential.status = BridgeCredentialStatus::Revoked;
            self.save_bridge_session_credential(&credential)?;
        }
        Ok(())
    }

    pub fn next_bridge_rotation_epoch(
        &self,
        bridge_instance_id: &str,
        adapter_id: &str,
    ) -> Result<u32> {
        let conn = self.connection()?;
        let epoch = conn.query_row(
            "
            SELECT COALESCE(MAX(rotation_epoch), 0)
            FROM bridge_session_credentials
            WHERE bridge_instance_id = ?1 AND adapter_id = ?2
            ",
            params![bridge_instance_id, adapter_id],
            |row| row.get::<_, u32>(0),
        )?;
        Ok(epoch + 1)
    }

    pub fn record_bridge_nonce(
        &self,
        secret_ref: &BridgeSecretRef,
        nonce: &str,
        signed_at: &str,
    ) -> Result<bool> {
        let conn = self.connection()?;
        let inserted = conn
            .execute(
                "
                INSERT INTO bridge_nonce_ledger (secret_ref, nonce, signed_at)
                VALUES (?1, ?2, ?3)
                ON CONFLICT(secret_ref, nonce) DO NOTHING
                ",
                params![secret_ref, nonce, signed_at],
            )
            .context("recording bridge nonce")?;
        Ok(inserted > 0)
    }

    pub fn append_bridge_auth_audit(
        &self,
        event_name: &str,
        payload: serde_json::Value,
    ) -> Result<()> {
        let created_at = now_timestamp();
        let record = serde_json::json!({
            "audit_id": new_id("bridge-audit"),
            "event_name": event_name,
            "created_at": created_at,
            "payload": payload,
        });
        let conn = self.connection()?;
        conn.execute(
            "
            INSERT INTO bridge_auth_audits (audit_id, event_name, created_at, payload_json)
            VALUES (?1, ?2, ?3, ?4)
            ",
            params![
                record["audit_id"].as_str().unwrap_or_default(),
                event_name,
                created_at,
                serde_json::to_string(&record)?,
            ],
        )
        .context("appending bridge auth audit")?;
        drop(conn);
        self.append_jsonl(
            self.runtime_root()
                .join(RUNTIME_BRIDGES_DIR)
                .join("bridge-auth")
                .join("audit.jsonl"),
            &record,
        )?;
        Ok(())
    }

    pub fn bridge_ticket_matches_secret(
        &self,
        ticket: &BridgeBootstrapTicket,
        ticket_secret: &str,
    ) -> bool {
        ticket.ticket_secret_hash == hash_secret(ticket_secret)
    }

    pub fn bridge_credential_matches_secret(
        &self,
        credential: &BridgeSessionCredential,
        session_secret: &str,
    ) -> bool {
        credential.secret_hash == hash_secret(session_secret)
    }
}
