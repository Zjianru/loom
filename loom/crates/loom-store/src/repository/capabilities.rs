use crate::LoomStore;
use anyhow::{Context, Result};
use loom_domain::{HostCapabilitySnapshot, HostSessionId};
use rusqlite::params;

impl LoomStore {
    pub fn save_capability_snapshot(&self, snapshot: &HostCapabilitySnapshot) -> Result<()> {
        let payload_json =
            serde_json::to_string(snapshot).context("serializing capability snapshot")?;
        let conn = self.connection()?;
        conn.execute(
            "
            INSERT INTO host_capability_snapshots (
                capability_snapshot_ref, host_session_id, recorded_at, payload_json
            ) VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(capability_snapshot_ref) DO UPDATE SET payload_json = excluded.payload_json
            ",
            params![
                snapshot.capability_snapshot_ref,
                snapshot.host_session_id,
                snapshot.recorded_at,
                payload_json
            ],
        )
        .context("upserting capability snapshot")?;
        Ok(())
    }

    pub fn latest_capability_snapshot(
        &self,
        host_session_id: &HostSessionId,
    ) -> Result<Option<HostCapabilitySnapshot>> {
        self.load_json_row(
            "
            SELECT payload_json
            FROM host_capability_snapshots
            WHERE host_session_id = ?1
            ORDER BY recorded_at DESC, rowid DESC
            LIMIT 1
            ",
            params![host_session_id],
        )
    }
}
