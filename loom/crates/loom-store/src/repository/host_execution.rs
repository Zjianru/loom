use crate::{LoomStore, RUNTIME_HOST_EXECUTION_DIR};
use anyhow::{Context, Result};
use loom_domain::{
    HostExecutionCommand, HostExecutionCommandId, HostExecutionCommandStatus, HostSessionId,
    HostSubagentLifecycleEnvelope, ManagedTaskRef, TaskEvent, new_id, now_timestamp,
};
use rusqlite::params;

impl LoomStore {
    pub fn save_host_execution_command(&self, command: &HostExecutionCommand) -> Result<()> {
        let payload_json =
            serde_json::to_string(command).context("serializing host execution command")?;
        let conn = self.connection()?;
        conn.execute(
            "
            INSERT INTO host_execution_commands (
                command_id, managed_task_ref, run_ref, host_session_id, role_kind, status, payload_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(command_id) DO UPDATE SET
                status = excluded.status,
                payload_json = excluded.payload_json
            ",
            params![
                command.command_id,
                command.managed_task_ref,
                command.run_ref,
                command.host_session_id,
                serde_json::to_string(&command.role_kind)?,
                serde_json::to_string(&command.status)?,
                payload_json,
            ],
        )
        .context("upserting host execution command")?;
        drop(conn);
        self.write_json(
            self.runtime_root()
                .join(RUNTIME_HOST_EXECUTION_DIR)
                .join("commands")
                .join(format!("{}.json", command.command_id)),
            command,
        )?;
        Ok(())
    }

    pub fn enqueue_host_execution_command(&self, command: &HostExecutionCommand) -> Result<()> {
        self.save_host_execution_command(command)
    }

    pub fn load_host_execution_command(
        &self,
        command_id: &HostExecutionCommandId,
    ) -> Result<Option<HostExecutionCommand>> {
        self.load_json_row(
            "SELECT payload_json FROM host_execution_commands WHERE command_id = ?1",
            params![command_id],
        )
    }

    pub fn next_host_execution_command(
        &self,
        host_session_id: &HostSessionId,
    ) -> Result<Option<HostExecutionCommand>> {
        self.load_json_row(
            "
            SELECT payload_json
            FROM host_execution_commands
            WHERE host_session_id = ?1 AND status = ?2
            ORDER BY rowid ASC
            LIMIT 1
            ",
            params![
                host_session_id,
                serde_json::to_string(&HostExecutionCommandStatus::Pending)?
            ],
        )
    }

    pub fn ack_host_execution_command(&self, command_id: &HostExecutionCommandId) -> Result<bool> {
        let Some(mut command) = self.load_host_execution_command(command_id)? else {
            return Ok(false);
        };
        if command.acked_at.is_some() {
            return Ok(true);
        }
        let should_emit_dispatched = command.status == HostExecutionCommandStatus::Pending;
        if command.status == HostExecutionCommandStatus::Pending {
            command.status = HostExecutionCommandStatus::Dispatched;
        }
        command.acked_at = Some(now_timestamp());
        self.save_host_execution_command(&command)?;
        if should_emit_dispatched {
            self.append_task_event(TaskEvent {
                event_id: new_id("event"),
                managed_task_ref: command.managed_task_ref.clone(),
                event_name: "host_execution_command_dispatched".into(),
                payload: serde_json::json!({
                    "managed_task_ref": command.managed_task_ref,
                    "command_id": command.command_id,
                    "binding_id": command.binding_id,
                    "role_kind": command.role_kind,
                    "host_session_id": command.host_session_id,
                    "host_agent_id": command.host_agent_id,
                    "host_child_execution_ref": command.host_child_execution_ref,
                    "host_child_run_ref": command.host_child_run_ref,
                    "artifact_refs": command.artifact_refs,
                    "terminal_status": serde_json::Value::Null,
                }),
                recorded_at: now_timestamp(),
            })?;
        }
        Ok(true)
    }

    pub fn list_host_execution_commands(
        &self,
        managed_task_ref: &ManagedTaskRef,
    ) -> Result<Vec<HostExecutionCommand>> {
        self.load_json_rows(
            "
            SELECT payload_json
            FROM host_execution_commands
            WHERE managed_task_ref = ?1
            ORDER BY rowid ASC
            ",
            params![managed_task_ref],
        )
    }

    pub fn save_host_subagent_lifecycle_event(
        &self,
        envelope: &HostSubagentLifecycleEnvelope,
    ) -> Result<()> {
        let payload_json =
            serde_json::to_string(envelope).context("serializing subagent lifecycle event")?;
        let conn = self.connection()?;
        conn.execute(
            "
            INSERT INTO host_subagent_lifecycle_events (
                ingress_id, command_id, managed_task_ref, run_ref, payload_json
            ) VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(ingress_id) DO UPDATE SET payload_json = excluded.payload_json
            ",
            params![
                envelope.meta.ingress_id,
                envelope.command_id,
                envelope.managed_task_ref,
                envelope.run_ref,
                payload_json,
            ],
        )
        .context("upserting subagent lifecycle event")?;
        drop(conn);
        self.write_json(
            self.runtime_root()
                .join(RUNTIME_HOST_EXECUTION_DIR)
                .join("lifecycle")
                .join(format!("{}.json", envelope.meta.ingress_id)),
            envelope,
        )?;
        Ok(())
    }

    pub fn list_host_subagent_lifecycle_events(
        &self,
        managed_task_ref: &ManagedTaskRef,
    ) -> Result<Vec<HostSubagentLifecycleEnvelope>> {
        self.load_json_rows(
            "
            SELECT payload_json
            FROM host_subagent_lifecycle_events
            WHERE managed_task_ref = ?1
            ORDER BY rowid ASC
            ",
            params![managed_task_ref],
        )
    }
}
