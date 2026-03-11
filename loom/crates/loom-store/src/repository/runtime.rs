use crate::{
    LoomStore, RUNTIME_AGENT_BINDINGS_DIR, RUNTIME_BOOTSTRAP_DIR, RUNTIME_BRIDGES_DIR,
    RUNTIME_EVENTS_DIR, RUNTIME_HOST_EXECUTION_DIR, RUNTIME_PHASE_PLANS_DIR,
    RUNTIME_PROJECTIONS_DIR, RUNTIME_RESULTS_DIR, RUNTIME_REVIEWS_DIR, RUNTIME_RUNS_DIR,
    RUNTIME_TASKS_DIR,
};
use anyhow::{Context, Result};
use std::fs;

impl LoomStore {
    pub fn ensure_runtime_layout(&self) -> Result<()> {
        for relative in [
            "",
            RUNTIME_TASKS_DIR,
            RUNTIME_EVENTS_DIR,
            RUNTIME_PROJECTIONS_DIR,
            &format!("{RUNTIME_PROJECTIONS_DIR}/tasks"),
            RUNTIME_RUNS_DIR,
            RUNTIME_PHASE_PLANS_DIR,
            RUNTIME_AGENT_BINDINGS_DIR,
            RUNTIME_REVIEWS_DIR,
            RUNTIME_RESULTS_DIR,
            RUNTIME_BRIDGES_DIR,
            &format!("{RUNTIME_BRIDGES_DIR}/outbound"),
            &format!("{RUNTIME_BRIDGES_DIR}/current-turns"),
            &format!("{RUNTIME_BRIDGES_DIR}/bridge-auth"),
            RUNTIME_HOST_EXECUTION_DIR,
            &format!("{RUNTIME_HOST_EXECUTION_DIR}/commands"),
            &format!("{RUNTIME_HOST_EXECUTION_DIR}/lifecycle"),
            RUNTIME_BOOTSTRAP_DIR,
        ] {
            fs::create_dir_all(self.runtime_root().join(relative))
                .with_context(|| format!("creating runtime directory {relative}"))?;
        }
        Ok(())
    }
}
