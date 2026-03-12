use crate::model::ids::{
    AgentBindingId, AgentBindingRef, DecisionToken, ExecutionAuthorizationId, HostSessionId,
    IsolatedTaskRunRef, ManagedTaskRef, PendingDecisionWindowId, PendingDecisionWindowRef,
    PhasePlanEntryId, PhasePlanId, ProofOfWorkBundleId, RequirementId, ResultContractId,
    ReviewResultId, RiskAssessmentId, SemanticDecisionEnvelopeId, SpecBundleId, TaskScopeId,
    Timestamp, new_id,
};
use crate::model::ingress::ControlActionKind;
use crate::model::outbound::{AcceptanceResult, ResultOutcome};
use crate::model::shared::{
    ManagedTaskClass, TaskActivationReason, WorkHorizonKind, WorkflowStage,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManagedTask {
    pub managed_task_ref: ManagedTaskRef,
    pub host_session_id: HostSessionId,
    pub managed_task_class: ManagedTaskClass,
    pub work_horizon: WorkHorizonKind,
    pub activation_reason: TaskActivationReason,
    pub workflow_stage: WorkflowStage,
    pub title: String,
    pub summary: String,
    pub expected_outcome: String,
    pub workspace_ref: Option<String>,
    pub repo_ref: Option<String>,
    pub allowed_roots: Vec<String>,
    pub secret_classes: Vec<String>,
    pub requirement_items: Vec<RequirementItem>,
    pub current_scope_ref: Option<TaskScopeId>,
    pub current_scope_version: Option<u32>,
    pub current_baseline_risk_ref: Option<RiskAssessmentId>,
    pub current_execution_authorization_ref: Option<ExecutionAuthorizationId>,
    pub current_pending_window_ref: Option<PendingDecisionWindowId>,
    pub active_run_ref: Option<IsolatedTaskRunRef>,
    pub spec_bundle: Option<SpecBundle>,
    #[serde(default)]
    pub phase_plan: Option<PhasePlan>,
    #[serde(default)]
    pub agent_binding: Option<AgentBinding>,
    pub review_result: Option<ReviewResult>,
    #[serde(default)]
    pub proof_of_work_bundle: Option<ProofOfWorkBundle>,
    pub result_contract: Option<ResultContract>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingDecisionWindow {
    pub window_id: PendingDecisionWindowId,
    pub managed_task_ref: ManagedTaskRef,
    pub kind: PendingDecisionWindowKind,
    pub decision_token: DecisionToken,
    #[serde(default)]
    pub source_decision_ref: Option<SemanticDecisionEnvelopeId>,
    pub status: PendingDecisionWindowStatus,
    pub allowed_actions: Vec<ControlActionKind>,
    pub opened_reason: String,
    pub opened_at: Timestamp,
    pub expires_at: Option<Timestamp>,
    pub supersedes: Option<PendingDecisionWindowRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PendingDecisionWindowKind {
    StartCandidate,
    ApprovalRequest,
    BoundaryConfirmation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PendingDecisionWindowStatus {
    Open,
    Consumed,
    Superseded,
    Expired,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingBoundaryConfirmation {
    pub window_ref: PendingDecisionWindowRef,
    pub active_managed_task_ref: ManagedTaskRef,
    pub boundary_candidate_ref: ManagedTaskRef,
    pub boundary_reason: BoundaryReason,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryReason {
    ExistingTaskActive,
    ExplicitReplacementRequested,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskScopeSnapshot {
    pub scope_id: TaskScopeId,
    pub managed_task_ref: ManagedTaskRef,
    pub scope_version: u32,
    pub scope_summary: String,
    pub requirement_items: Vec<RequirementItem>,
    pub workspace_ref: Option<String>,
    pub repo_ref: Option<String>,
    pub allowed_roots: Vec<String>,
    pub secret_classes: Vec<String>,
    pub constraints: Vec<ScopeConstraint>,
    pub assumptions: Vec<ScopeAssumption>,
    pub source_decision_ref: SemanticDecisionEnvelopeId,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RequirementItem {
    pub requirement_id: RequirementId,
    pub text: String,
    pub status: RequirementStatus,
    pub origin: RequirementOrigin,
    pub superseded_by: Option<RequirementId>,
    pub deferred_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RequirementStatus {
    Accepted,
    InProgress,
    Completed,
    Superseded,
    Deferred,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RequirementOrigin {
    InitialDecision,
    TaskChange,
    UserClarification,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScopeConstraint {
    pub label: String,
    pub details: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScopeAssumption {
    pub label: String,
    pub details: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IsolatedTaskRun {
    pub run_ref: IsolatedTaskRunRef,
    pub managed_task_ref: ManagedTaskRef,
    pub status: IsolatedTaskRunStatus,
    pub spec_bundle: SpecBundle,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IsolatedTaskRunStatus {
    Draft,
    Active,
    Review,
    Result,
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpecBundle {
    pub spec_bundle_id: SpecBundleId,
    pub managed_task_ref: ManagedTaskRef,
    pub task_scope_ref: TaskScopeId,
    pub summary: String,
    pub scope_doc: String,
    pub plan_doc: String,
    pub verification_doc: String,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewResult {
    pub review_result_id: ReviewResultId,
    pub managed_task_ref: ManagedTaskRef,
    pub run_ref: IsolatedTaskRunRef,
    pub reviewer_group_ref: String,
    pub review_verdict: ReviewVerdict,
    pub findings: Vec<String>,
    pub review_artifacts: Vec<ArtifactManifestItem>,
    pub summary: ReviewSummary,
    pub reviewed_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResultContract {
    pub result_contract_id: ResultContractId,
    pub managed_task_ref: ManagedTaskRef,
    pub pack_ref: Option<String>,
    pub outcome: ResultOutcome,
    pub acceptance_verdict: AcceptanceResult,
    pub final_scope_version: u32,
    pub scope_revision_summary: Option<String>,
    pub summary: String,
    pub key_outcomes: Vec<String>,
    pub proof_of_work: ProofOfWorkBundle,
    pub next_actions: Vec<NextActionItem>,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PhasePlan {
    pub phase_plan_id: PhasePlanId,
    pub managed_task_ref: ManagedTaskRef,
    pub plan_source: PhasePlanSource,
    pub plan_entries: Vec<PhasePlanEntry>,
    pub mutation_policy: PhasePlanMutationPolicy,
    pub metadata: PhasePlanMetadata,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PhasePlanEntry {
    pub entry_id: PhasePlanEntryId,
    pub stage_package_id: String,
    pub sequence_no: u16,
    pub visibility: StageVisibility,
    pub origin: PhaseEntryOrigin,
    pub required: bool,
    pub skip_allowed: bool,
    pub rework_target: Option<PhasePlanEntryId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PhasePlanSource {
    SystemDefault,
    UserAdjusted,
    SystemMutated,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StageVisibility {
    UserVisible,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PhaseEntryOrigin {
    PackDefault,
    UserSelected,
    SystemInserted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PhasePlanMutationPolicy {
    pub user_adjustment_allowed: bool,
    pub system_insert_allowed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PhasePlanMetadata {
    pub pack_ref: Option<String>,
    pub default_stage_sequence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentBinding {
    pub binding_id: AgentBindingId,
    pub managed_task_ref: ManagedTaskRef,
    pub run_ref: IsolatedTaskRunRef,
    pub stage_run_ref: Option<String>,
    pub pack_ref: Option<String>,
    pub capability_snapshot_ref: String,
    pub members: Vec<AgentBindingMember>,
    pub status: AgentBindingStatus,
    pub issued_reason: String,
    pub issued_at: Timestamp,
    pub supersedes: Option<AgentBindingRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentBindingMember {
    pub role_kind: AgentRoleKind,
    pub profile_ref: String,
    pub host_mapping_ref: Option<String>,
    pub responsibilities: Vec<String>,
    pub execution_mode: AgentExecutionMode,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentRoleKind {
    Net,
    Worker,
    Recorder,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentExecutionMode {
    Inline,
    BackgroundWorker,
    ReviewOnly,
    RecorderOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentBindingStatus {
    Draft,
    Active,
    Suspended,
    Superseded,
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewSummary {
    pub review_verdict: ReviewVerdict,
    pub summary: String,
    pub key_findings: Vec<String>,
    pub follow_up_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    Approved,
    ReworkRequired,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactManifestItem {
    pub label: String,
    pub reference: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NextActionItem {
    pub title: String,
    pub details: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProofOfWorkBundle {
    pub proof_of_work_id: ProofOfWorkBundleId,
    pub managed_task_ref: ManagedTaskRef,
    pub run_ref: IsolatedTaskRunRef,
    pub acceptance_verdict: AcceptanceResult,
    pub acceptance_basis: Vec<String>,
    pub accepted_by_ref: String,
    pub accepted_at: Timestamp,
    pub accepted_scope_version: u32,
    pub review_summary: ReviewSummary,
    pub artifact_manifest: Vec<ArtifactManifestItem>,
    pub evidence_refs: Vec<ProofEvidenceRef>,
    pub run_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProofEvidenceRef {
    pub label: String,
    pub reference: String,
}

impl RequirementItem {
    pub fn accepted(text: impl Into<String>, origin: RequirementOrigin) -> Self {
        Self {
            requirement_id: new_id("req"),
            text: text.into(),
            status: RequirementStatus::Accepted,
            origin,
            superseded_by: None,
            deferred_reason: None,
        }
    }
}
