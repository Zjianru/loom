use crate::{LoomStore, RUNTIME_EVENTS_DIR};
use anyhow::{Context, Result};
use loom_domain::{ManagedTaskRef, TaskEvent};
use rusqlite::params;

impl LoomStore {
    pub fn append_task_event(&self, event: TaskEvent) -> Result<()> {
        let payload_json = serde_json::to_string(&event).context("serializing task event")?;
        let conn = self.connection()?;
        conn.execute(
            "
            INSERT INTO task_events (event_id, managed_task_ref, event_name, recorded_at, payload_json)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ",
            params![
                event.event_id,
                event.managed_task_ref,
                event.event_name,
                event.recorded_at,
                payload_json
            ],
        )
        .context("appending task event")?;
        drop(conn);
        self.append_jsonl(
            self.runtime_root()
                .join(RUNTIME_EVENTS_DIR)
                .join(format!("{}.jsonl", event.managed_task_ref)),
            &event,
        )?;
        Ok(())
    }

    pub fn list_task_events(&self, managed_task_ref: &ManagedTaskRef) -> Result<Vec<TaskEvent>> {
        self.load_json_rows(
            "
            SELECT payload_json
            FROM task_events
            WHERE managed_task_ref = ?1
            ORDER BY sequence_id ASC
            ",
            params![managed_task_ref],
        )
    }
}
