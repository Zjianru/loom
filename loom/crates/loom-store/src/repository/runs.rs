use crate::{LoomStore, RUNTIME_RUNS_DIR};
use anyhow::{Context, Result};
use loom_domain::IsolatedTaskRun;
use rusqlite::params;

impl LoomStore {
    pub fn save_task_run(&self, run: &IsolatedTaskRun) -> Result<()> {
        let payload_json = serde_json::to_string(run).context("serializing task run")?;
        let conn = self.connection()?;
        conn.execute(
            "
            INSERT INTO isolated_task_runs (run_ref, managed_task_ref, status, payload_json)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(run_ref) DO UPDATE SET payload_json = excluded.payload_json, status = excluded.status
            ",
            params![
                run.run_ref,
                run.managed_task_ref,
                serde_json::to_string(&run.status)?,
                payload_json
            ],
        )
        .context("upserting task run")?;
        drop(conn);
        self.write_json(
            self.runtime_root()
                .join(RUNTIME_RUNS_DIR)
                .join(format!("{}.json", run.run_ref)),
            run,
        )?;
        Ok(())
    }

    pub fn load_task_run(&self, run_ref: &str) -> Result<Option<IsolatedTaskRun>> {
        self.load_json_row(
            "SELECT payload_json FROM isolated_task_runs WHERE run_ref = ?1",
            params![run_ref],
        )
    }
}
