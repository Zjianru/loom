use crate::model::authorization::AuthorizationBudgetBand;
use crate::model::ids::{
    ControlActionId, DecisionToken, HostCapabilitySnapshotRef, HostMessageRef, HostSessionId,
    HostExecutionCommandId, IsolatedTaskRunRef, ManagedTaskRef, SemanticDecisionEnvelopeId,
    Timestamp, new_id, now_timestamp,
};
use crate::model::shared::{
    InteractionLane, ManagedTaskClass, TaskActivationReason, WorkHorizonKind,
};
use crate::model::task::{AgentRoleKind, RequirementOrigin};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IngressMeta {
    pub ingress_id: String,
    pub received_at: Timestamp,
    pub causation_id: Option<String>,
    pub correlation_id: String,
    pub dedupe_window: String,
}

impl Default for IngressMeta {
    fn default() -> Self {
        Self {
            ingress_id: new_id("ingress"),
            received_at: now_timestamp(),
            causation_id: None,
            correlation_id: new_id("corr"),
            dedupe_window: "PT10M".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CurrentTurnEnvelope {
    pub meta: IngressMeta,
    pub host_session_id: HostSessionId,
    pub host_message_ref: Option<HostMessageRef>,
    pub text: String,
    pub workspace_ref: Option<String>,
    pub repo_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostCapabilitySnapshot {
    pub capability_snapshot_ref: HostCapabilitySnapshotRef,
    pub host_session_id: HostSessionId,
    pub allowed_tools: Vec<String>,
    pub readable_roots: Vec<String>,
    pub writable_roots: Vec<String>,
    pub secret_classes: Vec<String>,
    pub max_budget_band: AuthorizationBudgetBand,
    #[serde(default)]
    pub available_agent_ids: Vec<String>,
    #[serde(default)]
    pub supports_spawn_agents: bool,
    pub supports_pause: bool,
    pub supports_resume: bool,
    pub supports_interrupt: bool,
    pub recorded_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RequirementItemDraft {
    pub text: String,
    pub origin: RequirementOrigin,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticDecisionEnvelope {
    pub decision_id: SemanticDecisionEnvelopeId,
    pub host_session_id: HostSessionId,
    pub host_message_ref: Option<HostMessageRef>,
    pub managed_task_ref: Option<ManagedTaskRef>,
    pub interaction_lane: InteractionLane,
    pub managed_task_class: Option<ManagedTaskClass>,
    pub work_horizon: Option<WorkHorizonKind>,
    pub task_activation_reason: Option<TaskActivationReason>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub expected_outcome: Option<String>,
    pub requirement_items: Vec<RequirementItemDraft>,
    pub workspace_ref: Option<String>,
    pub repo_ref: Option<String>,
    pub allowed_roots: Vec<String>,
    pub secret_classes: Vec<String>,
    pub confidence: Option<u8>,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ControlActionKind {
    ApproveStart,
    ModifyCandidate,
    CancelCandidate,
    KeepCurrentTask,
    ReplaceActive,
    ApproveRequest,
    RejectRequest,
    PauseTask,
    ResumeTask,
    CancelTask,
    RequestReview,
    RequestHorizonReconsideration,
    RequestTaskChange,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ControlActorRef {
    User,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ControlActionPayload {
    pub title: Option<String>,
    pub summary: Option<String>,
    pub expected_outcome: Option<String>,
    pub requirement_items: Vec<RequirementItemDraft>,
    pub allowed_roots: Vec<String>,
    pub secret_classes: Vec<String>,
    pub workspace_ref: Option<String>,
    pub repo_ref: Option<String>,
    pub rationale: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ControlAction {
    pub action_id: ControlActionId,
    pub managed_task_ref: Option<ManagedTaskRef>,
    pub kind: ControlActionKind,
    pub actor: ControlActorRef,
    pub payload: ControlActionPayload,
    pub source_decision_ref: Option<SemanticDecisionEnvelopeId>,
    pub decision_token: Option<DecisionToken>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostSubagentLifecycleEnvelope {
    pub meta: IngressMeta,
    pub command_id: HostExecutionCommandId,
    pub managed_task_ref: ManagedTaskRef,
    pub run_ref: IsolatedTaskRunRef,
    pub role_kind: AgentRoleKind,
    pub event: HostSubagentLifecycleEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HostSubagentLifecycleEvent {
    Spawned(SubagentSpawnedPayload),
    Ended(SubagentEndedPayload),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubagentSpawnedPayload {
    #[serde(alias = "child_session_key")]
    pub host_child_execution_ref: String,
    #[serde(default, alias = "child_run_id")]
    pub host_child_run_ref: Option<String>,
    pub host_agent_id: String,
    pub observed_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubagentEndedPayload {
    #[serde(alias = "child_session_key")]
    pub host_child_execution_ref: String,
    #[serde(default, alias = "child_run_id")]
    pub host_child_run_ref: Option<String>,
    pub host_agent_id: String,
    pub status: HostSubagentStatus,
    pub output_summary: String,
    pub artifact_refs: Vec<String>,
    pub observed_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HostSubagentStatus {
    Completed,
    Failed,
    TimedOut,
    Cancelled,
}
