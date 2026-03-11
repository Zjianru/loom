use crate::LoomStore;
use anyhow::{Context, Result};
use rusqlite::{OptionalExtension, params};

impl LoomStore {
    pub fn table_exists(&self, table_name: &str) -> Result<bool> {
        let conn = self.connection()?;
        let exists: Option<String> = conn
            .query_row(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?1",
                params![table_name],
                |row| row.get(0),
            )
            .optional()
            .context("querying sqlite_master")?;
        Ok(exists.is_some())
    }

    pub(crate) fn init_schema(&self) -> Result<()> {
        let conn = self.connection()?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS managed_tasks (
                managed_task_ref TEXT PRIMARY KEY,
                host_session_id TEXT NOT NULL,
                workflow_stage TEXT NOT NULL,
                managed_task_class TEXT NOT NULL,
                work_horizon TEXT NOT NULL,
                current_scope_version INTEGER,
                current_pending_window_ref TEXT,
                current_execution_authorization_ref TEXT,
                current_baseline_risk_ref TEXT,
                active_run_ref TEXT,
                payload_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS task_scope_snapshots (
                scope_id TEXT PRIMARY KEY,
                managed_task_ref TEXT NOT NULL,
                scope_version INTEGER NOT NULL,
                payload_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS risk_assessments (
                assessment_id TEXT PRIMARY KEY,
                managed_task_ref TEXT NOT NULL,
                subject_kind TEXT NOT NULL,
                overall_risk_band TEXT NOT NULL,
                supersedes TEXT,
                payload_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS execution_authorizations (
                authorization_id TEXT PRIMARY KEY,
                managed_task_ref TEXT NOT NULL,
                capability_snapshot_ref TEXT NOT NULL,
                task_scope_ref TEXT NOT NULL,
                status TEXT NOT NULL,
                supersedes TEXT,
                payload_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS pending_decision_windows (
                window_id TEXT PRIMARY KEY,
                managed_task_ref TEXT NOT NULL,
                decision_token TEXT NOT NULL UNIQUE,
                kind TEXT NOT NULL,
                status TEXT NOT NULL,
                payload_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS host_capability_snapshots (
                capability_snapshot_ref TEXT PRIMARY KEY,
                host_session_id TEXT NOT NULL,
                recorded_at TEXT NOT NULL,
                payload_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS isolated_task_runs (
                run_ref TEXT PRIMARY KEY,
                managed_task_ref TEXT NOT NULL,
                status TEXT NOT NULL,
                payload_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS phase_plans (
                phase_plan_id TEXT PRIMARY KEY,
                managed_task_ref TEXT NOT NULL,
                plan_source TEXT NOT NULL,
                payload_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS agent_bindings (
                binding_id TEXT PRIMARY KEY,
                managed_task_ref TEXT NOT NULL,
                run_ref TEXT NOT NULL,
                status TEXT NOT NULL,
                payload_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS review_results (
                review_result_id TEXT PRIMARY KEY,
                managed_task_ref TEXT NOT NULL,
                run_ref TEXT NOT NULL,
                review_verdict TEXT NOT NULL,
                payload_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS proof_of_work_bundles (
                proof_of_work_id TEXT PRIMARY KEY,
                managed_task_ref TEXT NOT NULL,
                run_ref TEXT NOT NULL,
                acceptance_verdict TEXT NOT NULL,
                payload_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS result_contracts (
                result_contract_id TEXT PRIMARY KEY,
                managed_task_ref TEXT NOT NULL,
                outcome TEXT NOT NULL,
                acceptance_verdict TEXT NOT NULL,
                payload_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS host_execution_commands (
                command_id TEXT PRIMARY KEY,
                managed_task_ref TEXT NOT NULL,
                run_ref TEXT NOT NULL,
                host_session_id TEXT NOT NULL,
                role_kind TEXT NOT NULL,
                status TEXT NOT NULL,
                payload_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS host_subagent_lifecycle_events (
                ingress_id TEXT PRIMARY KEY,
                command_id TEXT NOT NULL,
                managed_task_ref TEXT NOT NULL,
                run_ref TEXT NOT NULL,
                payload_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS outbound_deliveries (
                sequence_id INTEGER PRIMARY KEY AUTOINCREMENT,
                delivery_id TEXT NOT NULL UNIQUE,
                host_session_id TEXT NOT NULL,
                managed_task_ref TEXT,
                correlation_id TEXT NOT NULL,
                causation_id TEXT,
                delivery_status TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                attempts INTEGER NOT NULL,
                max_attempts INTEGER NOT NULL,
                next_attempt_at TEXT,
                expires_at TEXT,
                last_error TEXT,
                created_at TEXT NOT NULL,
                acked_at TEXT
            );
            CREATE TABLE IF NOT EXISTS ingress_receipts (
                ingress_id TEXT NOT NULL,
                dedupe_window TEXT NOT NULL,
                ingress_kind TEXT NOT NULL,
                correlation_id TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                received_at TEXT NOT NULL,
                PRIMARY KEY (ingress_id, dedupe_window)
            );
            CREATE TABLE IF NOT EXISTS current_turns (
                sequence_id INTEGER PRIMARY KEY AUTOINCREMENT,
                ingress_id TEXT NOT NULL UNIQUE,
                host_session_id TEXT NOT NULL,
                host_message_ref TEXT,
                correlation_id TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                received_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS bridge_bootstrap_tickets (
                ticket_id TEXT PRIMARY KEY,
                bridge_instance_id TEXT NOT NULL,
                adapter_id TEXT NOT NULL,
                status TEXT NOT NULL,
                expires_at TEXT NOT NULL,
                payload_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS bridge_session_credentials (
                credential_id TEXT PRIMARY KEY,
                bridge_instance_id TEXT NOT NULL,
                adapter_id TEXT NOT NULL,
                secret_ref TEXT NOT NULL UNIQUE,
                rotation_epoch INTEGER NOT NULL,
                status TEXT NOT NULL,
                payload_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS bridge_nonce_ledger (
                secret_ref TEXT NOT NULL,
                nonce TEXT NOT NULL,
                signed_at TEXT NOT NULL,
                PRIMARY KEY (secret_ref, nonce)
            );
            CREATE TABLE IF NOT EXISTS bridge_auth_audits (
                audit_id TEXT PRIMARY KEY,
                event_name TEXT NOT NULL,
                created_at TEXT NOT NULL,
                payload_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS task_events (
                sequence_id INTEGER PRIMARY KEY AUTOINCREMENT,
                event_id TEXT NOT NULL UNIQUE,
                managed_task_ref TEXT NOT NULL,
                event_name TEXT NOT NULL,
                recorded_at TEXT NOT NULL,
                payload_json TEXT NOT NULL
            );
            ",
        )
        .context("initializing sqlite schema")?;
        Ok(())
    }
}
