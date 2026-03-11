mod repository;

use anyhow::{Context, Result, anyhow};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub(crate) const RUNTIME_TASKS_DIR: &str = "tasks";
pub(crate) const RUNTIME_EVENTS_DIR: &str = "events";
pub(crate) const RUNTIME_PROJECTIONS_DIR: &str = "projections";
pub(crate) const RUNTIME_RUNS_DIR: &str = "runs";
pub(crate) const RUNTIME_BRIDGES_DIR: &str = "host-bridges/openclaw";
pub(crate) const RUNTIME_BOOTSTRAP_DIR: &str = "bootstrap/openclaw";
pub(crate) const RUNTIME_PHASE_PLANS_DIR: &str = "phase-plans";
pub(crate) const RUNTIME_AGENT_BINDINGS_DIR: &str = "agent-bindings";
pub(crate) const RUNTIME_REVIEWS_DIR: &str = "reviews";
pub(crate) const RUNTIME_RESULTS_DIR: &str = "results";
pub(crate) const RUNTIME_HOST_EXECUTION_DIR: &str = "host-bridges/openclaw/host-execution";

#[derive(Clone)]
pub struct LoomStore {
    inner: Arc<StoreInner>,
}

struct StoreInner {
    conn: Mutex<Connection>,
    runtime_root: PathBuf,
}

impl LoomStore {
    pub fn open(database_path: impl AsRef<Path>, runtime_root: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(database_path.as_ref()).with_context(|| {
            format!(
                "opening sqlite database {}",
                database_path.as_ref().display()
            )
        })?;
        Self::from_connection(conn, runtime_root)
    }

    pub fn in_memory(runtime_root: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open_in_memory().context("opening in-memory sqlite database")?;
        Self::from_connection(conn, runtime_root)
    }

    fn from_connection(conn: Connection, runtime_root: impl AsRef<Path>) -> Result<Self> {
        let runtime_root = runtime_root.as_ref().to_path_buf();
        let store = Self {
            inner: Arc::new(StoreInner {
                conn: Mutex::new(conn),
                runtime_root,
            }),
        };
        store.ensure_runtime_layout()?;
        store.init_schema()?;
        Ok(store)
    }

    pub fn runtime_root(&self) -> &Path {
        &self.inner.runtime_root
    }

    fn connection(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.inner
            .conn
            .lock()
            .map_err(|_| anyhow!("sqlite connection mutex poisoned"))
    }

    fn load_json_row<T: for<'de> serde::Deserialize<'de>>(
        &self,
        sql: &str,
        params: impl rusqlite::Params,
    ) -> Result<Option<T>> {
        let conn = self.connection()?;
        let payload = conn
            .query_row(sql, params, |row| row.get::<_, String>(0))
            .optional()
            .context("querying json row")?;
        payload
            .map(|json| serde_json::from_str::<T>(&json).context("deserializing json row"))
            .transpose()
    }

    fn load_json_rows<T: for<'de> serde::Deserialize<'de>>(
        &self,
        sql: &str,
        params: impl rusqlite::Params,
    ) -> Result<Vec<T>> {
        let conn = self.connection()?;
        let mut statement = conn.prepare(sql).context("preparing sqlite statement")?;
        let rows = statement
            .query_map(params, |row| row.get::<_, String>(0))
            .context("querying sqlite rows")?;
        let mut values = Vec::new();
        for row in rows {
            let row = row.context("reading sqlite row")?;
            values.push(serde_json::from_str(&row).context("deserializing json rows")?);
        }
        Ok(values)
    }

    fn write_json<T: Serialize>(&self, path: PathBuf, value: &T) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating parent directory {}", parent.display()))?;
        }
        let payload = serde_json::to_string_pretty(value).context("serializing projection json")?;
        fs::write(&path, payload).with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    fn append_jsonl<T: Serialize>(&self, path: PathBuf, value: &T) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating parent directory {}", parent.display()))?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("opening {}", path.display()))?;
        let line = serde_json::to_string(value).context("serializing jsonl line")?;
        writeln!(file, "{line}").with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::LoomStore;
    use tempfile::tempdir;

    #[test]
    fn runtime_layout_is_exported_without_handoff_or_attention_dirs() {
        let dir = tempdir().expect("tempdir");
        let store = LoomStore::in_memory(dir.path()).expect("store");
        assert!(store.runtime_root().join("tasks").exists());
        assert!(store.runtime_root().join("events").exists());
        assert!(store.runtime_root().join("projections").exists());
        assert!(store.runtime_root().join("runs").exists());
        assert!(store.runtime_root().join("host-bridges/openclaw").exists());
        assert!(store.runtime_root().join("phase-plans").exists());
        assert!(store.runtime_root().join("agent-bindings").exists());
        assert!(store.runtime_root().join("reviews").exists());
        assert!(store.runtime_root().join("results").exists());
        assert!(
            store
                .runtime_root()
                .join("host-bridges/openclaw/host-execution")
                .exists()
        );
        assert!(!store.runtime_root().join("handoffs").exists());
        assert!(!store.runtime_root().join("attention").exists());
    }

    #[test]
    fn schema_freezes_without_handoff_or_attention_tables() {
        let dir = tempdir().expect("tempdir");
        let store = LoomStore::in_memory(dir.path()).expect("store");
        assert!(store.table_exists("managed_tasks").expect("managed_tasks"));
        assert!(
            store
                .table_exists("task_scope_snapshots")
                .expect("task_scope_snapshots")
        );
        assert!(
            store
                .table_exists("risk_assessments")
                .expect("risk_assessments")
        );
        assert!(
            store
                .table_exists("execution_authorizations")
                .expect("execution_authorizations")
        );
        assert!(store.table_exists("phase_plans").expect("phase_plans"));
        assert!(store.table_exists("agent_bindings").expect("agent_bindings"));
        assert!(store.table_exists("review_results").expect("review_results"));
        assert!(
            store
                .table_exists("proof_of_work_bundles")
                .expect("proof_of_work_bundles")
        );
        assert!(
            store
                .table_exists("result_contracts")
                .expect("result_contracts")
        );
        assert!(
            store
                .table_exists("host_execution_commands")
                .expect("host_execution_commands")
        );
        assert!(
            store
                .table_exists("host_subagent_lifecycle_events")
                .expect("host_subagent_lifecycle_events")
        );
        assert!(
            !store
                .table_exists("handoff_contracts")
                .expect("handoff_contracts")
        );
        assert!(
            !store
                .table_exists("attention_policies")
                .expect("attention_policies")
        );
    }
}
