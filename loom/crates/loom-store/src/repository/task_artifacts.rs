use crate::{
    LoomStore, RUNTIME_AGENT_BINDINGS_DIR, RUNTIME_PHASE_PLANS_DIR, RUNTIME_RESULTS_DIR,
    RUNTIME_REVIEWS_DIR,
};
use anyhow::{Context, Result};
use loom_domain::{
    AgentBinding, ManagedTaskRef, PhasePlan, ProofOfWorkBundle, ResultContract, ReviewResult,
};
use rusqlite::params;

impl LoomStore {
    pub fn save_phase_plan(&self, plan: &PhasePlan) -> Result<()> {
        let payload_json = serde_json::to_string(plan).context("serializing phase plan")?;
        let conn = self.connection()?;
        conn.execute(
            "
            INSERT INTO phase_plans (phase_plan_id, managed_task_ref, plan_source, payload_json)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(phase_plan_id) DO UPDATE SET payload_json = excluded.payload_json
            ",
            params![
                plan.phase_plan_id,
                plan.managed_task_ref,
                serde_json::to_string(&plan.plan_source)?,
                payload_json,
            ],
        )
        .context("upserting phase plan")?;
        drop(conn);
        self.write_json(
            self.runtime_root()
                .join(RUNTIME_PHASE_PLANS_DIR)
                .join(format!("{}.json", plan.phase_plan_id)),
            plan,
        )?;
        Ok(())
    }

    pub fn latest_phase_plan(&self, managed_task_ref: &ManagedTaskRef) -> Result<Option<PhasePlan>> {
        self.load_json_row(
            "
            SELECT payload_json
            FROM phase_plans
            WHERE managed_task_ref = ?1
            ORDER BY rowid DESC
            LIMIT 1
            ",
            params![managed_task_ref],
        )
    }

    pub fn save_agent_binding(&self, binding: &AgentBinding) -> Result<()> {
        let payload_json = serde_json::to_string(binding).context("serializing agent binding")?;
        let conn = self.connection()?;
        conn.execute(
            "
            INSERT INTO agent_bindings (binding_id, managed_task_ref, run_ref, status, payload_json)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(binding_id) DO UPDATE SET payload_json = excluded.payload_json, status = excluded.status
            ",
            params![
                binding.binding_id,
                binding.managed_task_ref,
                binding.run_ref,
                serde_json::to_string(&binding.status)?,
                payload_json,
            ],
        )
        .context("upserting agent binding")?;
        drop(conn);
        self.write_json(
            self.runtime_root()
                .join(RUNTIME_AGENT_BINDINGS_DIR)
                .join(format!("{}.json", binding.binding_id)),
            binding,
        )?;
        Ok(())
    }

    pub fn latest_agent_binding(
        &self,
        managed_task_ref: &ManagedTaskRef,
    ) -> Result<Option<AgentBinding>> {
        self.load_json_row(
            "
            SELECT payload_json
            FROM agent_bindings
            WHERE managed_task_ref = ?1
            ORDER BY rowid DESC
            LIMIT 1
            ",
            params![managed_task_ref],
        )
    }

    pub fn save_review_result(&self, review: &ReviewResult) -> Result<()> {
        let payload_json = serde_json::to_string(review).context("serializing review result")?;
        let conn = self.connection()?;
        conn.execute(
            "
            INSERT INTO review_results (
                review_result_id, managed_task_ref, run_ref, review_verdict, payload_json
            ) VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(review_result_id) DO UPDATE SET payload_json = excluded.payload_json
            ",
            params![
                review.review_result_id,
                review.managed_task_ref,
                review.run_ref,
                serde_json::to_string(&review.review_verdict)?,
                payload_json,
            ],
        )
        .context("upserting review result")?;
        drop(conn);
        self.write_json(
            self.runtime_root()
                .join(RUNTIME_REVIEWS_DIR)
                .join(format!("{}.json", review.review_result_id)),
            review,
        )?;
        Ok(())
    }

    pub fn latest_review_result(
        &self,
        managed_task_ref: &ManagedTaskRef,
    ) -> Result<Option<ReviewResult>> {
        self.load_json_row(
            "
            SELECT payload_json
            FROM review_results
            WHERE managed_task_ref = ?1
            ORDER BY rowid DESC
            LIMIT 1
            ",
            params![managed_task_ref],
        )
    }

    pub fn save_proof_of_work_bundle(&self, proof: &ProofOfWorkBundle) -> Result<()> {
        let payload_json =
            serde_json::to_string(proof).context("serializing proof of work bundle")?;
        let conn = self.connection()?;
        conn.execute(
            "
            INSERT INTO proof_of_work_bundles (
                proof_of_work_id, managed_task_ref, run_ref, acceptance_verdict, payload_json
            ) VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(proof_of_work_id) DO UPDATE SET payload_json = excluded.payload_json
            ",
            params![
                proof.proof_of_work_id,
                proof.managed_task_ref,
                proof.run_ref,
                serde_json::to_string(&proof.acceptance_verdict)?,
                payload_json,
            ],
        )
        .context("upserting proof of work bundle")?;
        drop(conn);
        self.write_json(
            self.runtime_root()
                .join(RUNTIME_RESULTS_DIR)
                .join(format!("proof-{}.json", proof.proof_of_work_id)),
            proof,
        )?;
        Ok(())
    }

    pub fn latest_proof_of_work_bundle(
        &self,
        managed_task_ref: &ManagedTaskRef,
    ) -> Result<Option<ProofOfWorkBundle>> {
        self.load_json_row(
            "
            SELECT payload_json
            FROM proof_of_work_bundles
            WHERE managed_task_ref = ?1
            ORDER BY rowid DESC
            LIMIT 1
            ",
            params![managed_task_ref],
        )
    }

    pub fn save_result_contract(&self, contract: &ResultContract) -> Result<()> {
        let payload_json = serde_json::to_string(contract).context("serializing result contract")?;
        let conn = self.connection()?;
        conn.execute(
            "
            INSERT INTO result_contracts (
                result_contract_id, managed_task_ref, outcome, acceptance_verdict, payload_json
            ) VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(result_contract_id) DO UPDATE SET payload_json = excluded.payload_json
            ",
            params![
                contract.result_contract_id,
                contract.managed_task_ref,
                serde_json::to_string(&contract.outcome)?,
                serde_json::to_string(&contract.acceptance_verdict)?,
                payload_json,
            ],
        )
        .context("upserting result contract")?;
        drop(conn);
        self.write_json(
            self.runtime_root()
                .join(RUNTIME_RESULTS_DIR)
                .join(format!("result-{}.json", contract.result_contract_id)),
            contract,
        )?;
        Ok(())
    }

    pub fn latest_result_contract(
        &self,
        managed_task_ref: &ManagedTaskRef,
    ) -> Result<Option<ResultContract>> {
        self.load_json_row(
            "
            SELECT payload_json
            FROM result_contracts
            WHERE managed_task_ref = ?1
            ORDER BY rowid DESC
            LIMIT 1
            ",
            params![managed_task_ref],
        )
    }
}
