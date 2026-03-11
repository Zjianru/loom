use crate::model::authorization::DecisionArea;
use crate::model::ids::{
    DecisionToken, HostMessageRef, HostSessionId, ManagedTaskRef, OutboundDeliveryId, PackRef,
    Timestamp,
};
use crate::model::shared::{ManagedTaskClass, TaskActivationReason, WorkHorizonKind};
use crate::model::task::{
    ArtifactManifestItem, BoundaryReason, NextActionItem, ProofEvidenceRef, ReviewSummary,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KernelOutboundPayload {
    StartCard(StartCardPayload),
    BoundaryCard(BoundaryCardPayload),
    ResultSummary(ResultSummaryPayload),
    ApprovalRequest(ApprovalRequestPayload),
    SuppressHostMessage(SuppressHostMessagePayload),
    ToolDecision(ToolDecisionPayload),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RenderHint {
    pub tone: RenderTone,
    pub emphasis: RenderEmphasis,
    pub host_text_mode: HostTextMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RenderTone {
    Neutral,
    Cautionary,
    Blocking,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RenderEmphasis {
    Minimal,
    Standard,
    Strong,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HostTextMode {
    Plain,
    StructuredText,
}

impl Default for RenderHint {
    fn default() -> Self {
        Self {
            tone: RenderTone::Neutral,
            emphasis: RenderEmphasis::Standard,
            host_text_mode: HostTextMode::StructuredText,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StartCardPayload {
    pub managed_task_ref: ManagedTaskRef,
    pub decision_token: DecisionToken,
    pub managed_task_class: ManagedTaskClass,
    pub work_horizon: WorkHorizonKind,
    pub task_activation_reason: TaskActivationReason,
    pub title: String,
    pub summary: String,
    pub expected_outcome: String,
    pub recommended_pack_ref: Option<PackRef>,
    pub allowed_actions: Vec<StartCardAction>,
    pub render_hint: RenderHint,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StartCardAction {
    ApproveStart,
    ModifyCandidate,
    CancelCandidate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BoundaryCardPayload {
    pub managed_task_ref: ManagedTaskRef,
    pub candidate_managed_task_ref: ManagedTaskRef,
    pub decision_token: DecisionToken,
    pub active_task_summary: String,
    pub candidate_task_summary: String,
    pub boundary_reason: BoundaryReason,
    pub allowed_actions: Vec<BoundaryCardAction>,
    pub render_hint: RenderHint,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryCardAction {
    KeepCurrentTask,
    ReplaceActive,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalRequestPayload {
    pub managed_task_ref: ManagedTaskRef,
    pub decision_token: DecisionToken,
    pub approval_scope: ApprovalScope,
    pub allowed_actions: Vec<ApprovalRequestAction>,
    pub why_now: String,
    pub risk_summary: String,
    pub render_hint: RenderHint,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalScope {
    ToolExecution,
    TaskChangeConfirmation,
    CapabilityDriftResolution,
    ReviewResolution,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalRequestAction {
    ApproveRequest,
    RejectRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResultSummaryPayload {
    pub managed_task_ref: ManagedTaskRef,
    pub outcome: ResultOutcome,
    pub acceptance_verdict: AcceptanceResult,
    pub summary: String,
    pub final_scope_version: u32,
    pub scope_revision_headline: Option<String>,
    pub proof_of_work_excerpt: ProofOfWorkExcerpt,
    pub next_actions_excerpt: Vec<NextActionItem>,
    pub render_hint: RenderHint,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProofOfWorkExcerpt {
    pub run_summary: String,
    pub evidence_refs: Vec<ProofEvidenceRef>,
    pub review_summary: Option<ReviewSummary>,
    pub artifact_manifest_excerpt: Vec<ArtifactManifestItem>,
    pub acceptance_basis_excerpt: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResultOutcome {
    Completed,
    ReworkRequired,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AcceptanceResult {
    Accepted,
    ReworkRequired,
    EscalatedToUser,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SuppressHostMessagePayload {
    pub host_message_ref: Option<HostMessageRef>,
    pub reason: SuppressionReason,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SuppressionReason {
    InternalPrelude,
    InternalGovernanceTrace,
    ToolNoise,
    DuplicateHostMessage,
    ReplacedByStructuredPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolDecisionPayload {
    pub managed_task_ref: ManagedTaskRef,
    pub decision_value: ToolDecisionValue,
    pub decision_area: DecisionArea,
    pub summary: String,
    pub render_hint: RenderHint,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolDecisionValue {
    Allow,
    Deny,
    RequiresUserApproval,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutboundDelivery {
    pub delivery_id: OutboundDeliveryId,
    pub host_session_id: HostSessionId,
    pub managed_task_ref: Option<ManagedTaskRef>,
    pub correlation_id: String,
    pub causation_id: Option<String>,
    pub payload: KernelOutboundPayload,
    pub delivery_status: DeliveryStatus,
    pub attempts: u32,
    pub max_attempts: u32,
    pub next_attempt_at: Option<Timestamp>,
    pub expires_at: Option<Timestamp>,
    pub last_error: Option<String>,
    pub created_at: Timestamp,
    pub acked_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryStatus {
    Pending,
    Delivering,
    DeliveredUnacked,
    Acked,
    RetryScheduled,
    Expired,
    TerminalFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskEvent {
    pub event_id: String,
    pub managed_task_ref: ManagedTaskRef,
    pub event_name: String,
    pub payload: serde_json::Value,
    pub recorded_at: Timestamp,
}
