use crate::LoomStore;
use anyhow::{Context, Result};
use loom_domain::{ManagedTaskRef, RiskAssessment, RiskSubjectKind};
use rusqlite::params;

impl LoomStore {
    pub fn save_risk_assessment(&self, assessment: &RiskAssessment) -> Result<()> {
        let payload_json =
            serde_json::to_string(assessment).context("serializing risk assessment")?;
        let conn = self.connection()?;
        conn.execute(
            "
            INSERT INTO risk_assessments (
                assessment_id, managed_task_ref, subject_kind, overall_risk_band, supersedes, payload_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(assessment_id) DO UPDATE SET payload_json = excluded.payload_json
            ",
            params![
                assessment.assessment_id,
                assessment.managed_task_ref,
                serde_json::to_string(&assessment.subject_kind)?,
                serde_json::to_string(&assessment.overall_risk_band)?,
                assessment.supersedes,
                payload_json,
            ],
        )
        .context("upserting risk assessment")?;
        Ok(())
    }

    pub fn list_risk_assessments(
        &self,
        managed_task_ref: &ManagedTaskRef,
    ) -> Result<Vec<RiskAssessment>> {
        self.load_json_rows(
            "
            SELECT payload_json
            FROM risk_assessments
            WHERE managed_task_ref = ?1
            ORDER BY rowid ASC
            ",
            params![managed_task_ref],
        )
    }

    pub fn latest_task_baseline(
        &self,
        managed_task_ref: &ManagedTaskRef,
    ) -> Result<Option<RiskAssessment>> {
        self.load_json_row(
            "
            SELECT payload_json
            FROM risk_assessments
            WHERE managed_task_ref = ?1 AND subject_kind = ?2
            ORDER BY rowid DESC
            LIMIT 1
            ",
            params![
                managed_task_ref,
                serde_json::to_string(&RiskSubjectKind::TaskBaseline)?
            ],
        )
    }

    pub fn latest_action_override(
        &self,
        managed_task_ref: &ManagedTaskRef,
    ) -> Result<Option<RiskAssessment>> {
        self.load_json_row(
            "
            SELECT payload_json
            FROM risk_assessments
            WHERE managed_task_ref = ?1 AND subject_kind = ?2
            ORDER BY rowid DESC
            LIMIT 1
            ",
            params![
                managed_task_ref,
                serde_json::to_string(&RiskSubjectKind::ActionOverride)?
            ],
        )
    }
}
