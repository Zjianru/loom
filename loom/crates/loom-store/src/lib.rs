mod repository;
pub use repository::semantic_decisions::PersistOutcome;

use anyhow::{Context, Result, anyhow, bail};
use loom_domain::{new_id, now_timestamp};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

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
    failpoints: Mutex<Vec<String>>,
}

pub struct LoomStoreTx<'a> {
    store: &'a LoomStore,
    conn: MutexGuard<'a, Connection>,
    projections: Vec<ProjectionOp>,
    finished: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectionFailureRecord {
    pub failure_id: String,
    pub projection_kind: String,
    pub target_path: String,
    pub error_message: String,
    pub recorded_at: String,
}

#[derive(Debug, Clone)]
enum ProjectionOp {
    WriteJson { path: PathBuf, payload: String },
    AppendJsonl { path: PathBuf, line: String },
}

impl ProjectionOp {
    fn kind(&self) -> &'static str {
        match self {
            ProjectionOp::WriteJson { .. } => "write_json",
            ProjectionOp::AppendJsonl { .. } => "append_jsonl",
        }
    }

    fn target_path(&self) -> &Path {
        match self {
            ProjectionOp::WriteJson { path, .. } => path.as_path(),
            ProjectionOp::AppendJsonl { path, .. } => path.as_path(),
        }
    }
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
                failpoints: Mutex::new(Vec::new()),
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
        load_json_row_from_conn(&conn, sql, params)
    }

    fn load_json_rows<T: for<'de> serde::Deserialize<'de>>(
        &self,
        sql: &str,
        params: impl rusqlite::Params,
    ) -> Result<Vec<T>> {
        let conn = self.connection()?;
        load_json_rows_from_conn(&conn, sql, params)
    }

    fn write_json<T: Serialize>(&self, path: PathBuf, value: &T) -> Result<()> {
        let payload = serde_json::to_string_pretty(value).context("serializing projection json")?;
        write_json_payload(&path, &payload)
    }

    fn append_jsonl<T: Serialize>(&self, path: PathBuf, value: &T) -> Result<()> {
        let line = serde_json::to_string(value).context("serializing jsonl line")?;
        append_jsonl_payload(&path, &line)
    }

    pub fn begin_tx(&self) -> Result<LoomStoreTx<'_>> {
        let conn = self.connection()?;
        conn.execute_batch("BEGIN IMMEDIATE TRANSACTION")
            .context("beginning sqlite transaction")?;
        Ok(LoomStoreTx {
            store: self,
            conn,
            projections: Vec::new(),
            finished: false,
        })
    }

    pub fn inject_failpoint(&self, name: &str) {
        let mut guard = self
            .inner
            .failpoints
            .lock()
            .expect("store failpoints mutex poisoned");
        guard.push(name.to_string());
    }

    fn maybe_fail(&self, name: &str) -> Result<()> {
        let mut guard = self
            .inner
            .failpoints
            .lock()
            .map_err(|_| anyhow!("store failpoints mutex poisoned"))?;
        if let Some(position) = guard.iter().position(|value| value == name) {
            guard.remove(position);
            bail!("injected store failpoint: {name}");
        }
        Ok(())
    }

    pub fn count_projection_failures(&self) -> Result<u32> {
        let conn = self.connection()?;
        let count = conn
            .query_row("SELECT COUNT(*) FROM projection_failures", [], |row| {
                row.get::<_, u32>(0)
            })
            .context("counting projection failures")?;
        Ok(count)
    }

    pub fn list_projection_failures(&self) -> Result<Vec<ProjectionFailureRecord>> {
        self.load_json_rows(
            "
            SELECT payload_json
            FROM projection_failures
            ORDER BY sequence_id ASC
            ",
            [],
        )
    }
}

impl LoomStoreTx<'_> {
    pub fn runtime_root(&self) -> &Path {
        self.store.runtime_root()
    }

    pub fn commit(mut self) -> Result<()> {
        self.conn
            .execute_batch("COMMIT")
            .context("committing sqlite transaction")?;
        self.finished = true;
        let projections = std::mem::take(&mut self.projections);
        for projection in projections {
            if let Err(error) = self.flush_projection(&projection) {
                let failure = ProjectionFailureRecord {
                    failure_id: new_id("projection-failure"),
                    projection_kind: projection.kind().into(),
                    target_path: projection.target_path().display().to_string(),
                    error_message: error.to_string(),
                    recorded_at: now_timestamp(),
                };
                if let Err(record_error) = record_projection_failure_with_conn(&self.conn, &failure)
                {
                    eprintln!(
                        "failed to record projection failure for {}: {}",
                        failure.target_path, record_error
                    );
                }
            }
        }
        Ok(())
    }

    pub fn rollback(mut self) -> Result<()> {
        if !self.finished {
            self.conn
                .execute_batch("ROLLBACK")
                .context("rolling back sqlite transaction")?;
            self.finished = true;
        }
        Ok(())
    }

    pub(crate) fn stage_json<T: Serialize>(&mut self, path: PathBuf, value: &T) -> Result<()> {
        let payload = serde_json::to_string_pretty(value).context("serializing projection json")?;
        self.projections
            .push(ProjectionOp::WriteJson { path, payload });
        Ok(())
    }

    pub(crate) fn stage_jsonl<T: Serialize>(&mut self, path: PathBuf, value: &T) -> Result<()> {
        let line = serde_json::to_string(value).context("serializing jsonl line")?;
        self.projections
            .push(ProjectionOp::AppendJsonl { path, line });
        Ok(())
    }

    pub(crate) fn maybe_fail(&self, name: &str) -> Result<()> {
        self.store.maybe_fail(name)
    }

    fn flush_projection(&self, projection: &ProjectionOp) -> Result<()> {
        match projection {
            ProjectionOp::WriteJson { path, payload } => {
                self.store.maybe_fail("projection.write_json")?;
                write_json_payload(path, payload)
            }
            ProjectionOp::AppendJsonl { path, line } => {
                self.store.maybe_fail("projection.append_jsonl")?;
                append_jsonl_payload(path, line)
            }
        }
    }
}

impl Drop for LoomStoreTx<'_> {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.conn.execute_batch("ROLLBACK");
            self.finished = true;
        }
    }
}

pub(crate) fn load_json_row_from_conn<T: for<'de> serde::Deserialize<'de>>(
    conn: &Connection,
    sql: &str,
    params: impl rusqlite::Params,
) -> Result<Option<T>> {
    let payload = conn
        .query_row(sql, params, |row| row.get::<_, String>(0))
        .optional()
        .context("querying json row")?;
    payload
        .map(|json| serde_json::from_str::<T>(&json).context("deserializing json row"))
        .transpose()
}

pub(crate) fn load_json_rows_from_conn<T: for<'de> serde::Deserialize<'de>>(
    conn: &Connection,
    sql: &str,
    params: impl rusqlite::Params,
) -> Result<Vec<T>> {
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

fn write_json_payload(path: &Path, payload: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating parent directory {}", parent.display()))?;
    }
    fs::write(path, payload).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn append_jsonl_payload(path: &Path, line: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating parent directory {}", parent.display()))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("opening {}", path.display()))?;
    writeln!(file, "{line}").with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn record_projection_failure_with_conn(
    conn: &Connection,
    failure: &ProjectionFailureRecord,
) -> Result<()> {
    let payload_json =
        serde_json::to_string(failure).context("serializing projection failure record")?;
    conn.execute(
        "
        INSERT INTO projection_failures (
            failure_id, projection_kind, target_path, recorded_at, payload_json
        ) VALUES (?1, ?2, ?3, ?4, ?5)
        ",
        rusqlite::params![
            failure.failure_id,
            failure.projection_kind,
            failure.target_path,
            failure.recorded_at,
            payload_json,
        ],
    )
    .context("recording projection failure")?;
    Ok(())
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
        assert!(
            store
                .table_exists("agent_bindings")
                .expect("agent_bindings")
        );
        assert!(
            store
                .table_exists("review_results")
                .expect("review_results")
        );
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
