use crate::model::authorization::{AuthorizationBudgetBand, DecisionArea};
use crate::model::ids::{
    HostCapabilitySnapshotRef, ManagedTaskRef, RiskAssessmentId, RiskAssessmentRef, StageRunRef,
    Timestamp,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RiskAssessment {
    pub assessment_id: RiskAssessmentId,
    pub managed_task_ref: ManagedTaskRef,
    pub subject_kind: RiskSubjectKind,
    pub capability_snapshot_ref: Option<HostCapabilitySnapshotRef>,
    pub stage_run_ref: Option<StageRunRef>,
    pub decision_area: Option<DecisionArea>,
    pub overall_risk_band: RiskBand,
    pub risk_dimensions: RiskDimensions,
    pub trigger_reason: String,
    pub evidence_refs: Vec<EvidenceRef>,
    pub derived_consequences: Vec<RiskConsequence>,
    pub assessed_at: Timestamp,
    pub supersedes: Option<RiskAssessmentRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiskSubjectKind {
    TaskBaseline,
    ActionOverride,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum RiskBand {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RiskDimensions {
    pub write_impact: RiskBand,
    pub external_side_effect: RiskBand,
    pub secret_exposure: RiskBand,
    pub irreversibility: RiskBand,
    pub budget_burn: RiskBand,
    pub uncertainty: RiskBand,
}

impl RiskDimensions {
    pub fn max_band(&self) -> RiskBand {
        [
            self.write_impact,
            self.external_side_effect,
            self.secret_exposure,
            self.irreversibility,
            self.budget_burn,
            self.uncertainty,
        ]
        .into_iter()
        .max()
        .unwrap_or(RiskBand::Low)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiskConsequence {
    MountAdditionalGovernance,
    NarrowExecutionAuthorization,
    RequireApprovalWindow,
    RequireAdditionalReview,
    StopSilentExecution,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceRef {
    pub kind: String,
    pub reference: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProposedAction {
    pub proposal_id: String,
    pub managed_task_ref: ManagedTaskRef,
    pub stage_run_ref: Option<StageRunRef>,
    pub decision_area: DecisionArea,
    pub tool_name: Option<String>,
    pub readable_roots: Vec<String>,
    pub writable_roots: Vec<String>,
    pub secret_classes: Vec<String>,
    pub external_side_effect: bool,
    pub irreversible: bool,
    pub estimated_budget_band: AuthorizationBudgetBand,
    pub preview_available: bool,
    pub reason: String,
}
