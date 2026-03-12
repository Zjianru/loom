use crate::model::authorization::AuthorizationBudgetBand;
use crate::model::ids::{
    ControlActionId, DecisionToken, HostCapabilitySnapshotRef, HostExecutionCommandId,
    HostMessageRef, HostSessionId, IsolatedTaskRunRef, ManagedTaskRef, SemanticDecisionEnvelopeId,
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
#[serde(rename_all = "snake_case")]
pub enum TaskChangeClassification {
    SameTaskMinor,
    SameTaskMaterial,
    SameTaskStructural,
    BoundaryConflictCandidate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeExecutionSurface {
    FutureOnly,
    ActiveStage,
    CompletedScope,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryRecommendation {
    AbsorbChange,
    RequireConfirmation,
    OpenBoundaryConfirmation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecisionSource {
    HostModel,
    PackDefault,
    SystemReconsideration,
    UserControlAction,
    AdapterFallback,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SemanticDecisionKind {
    InteractionLane,
    TaskActivationReason,
    ManagedTaskClass,
    WorkHorizon,
    TaskChange,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InteractionLaneDecisionPayload {
    pub interaction_lane: InteractionLane,
    pub managed_task_ref: Option<ManagedTaskRef>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub expected_outcome: Option<String>,
    #[serde(default)]
    pub requirement_items: Vec<RequirementItemDraft>,
    pub workspace_ref: Option<String>,
    pub repo_ref: Option<String>,
    #[serde(default)]
    pub allowed_roots: Vec<String>,
    #[serde(default)]
    pub secret_classes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskActivationReasonDecisionPayload {
    pub task_activation_reason: TaskActivationReason,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManagedTaskClassDecisionPayload {
    pub managed_task_class: ManagedTaskClass,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkHorizonDecisionPayload {
    pub work_horizon: WorkHorizonKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskChangeDecisionPayload {
    pub classification: TaskChangeClassification,
    pub execution_surface: ChangeExecutionSurface,
    pub boundary_recommendation: BoundaryRecommendation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum SemanticDecisionPayload {
    InteractionLane(InteractionLaneDecisionPayload),
    TaskActivationReason(TaskActivationReasonDecisionPayload),
    ManagedTaskClass(ManagedTaskClassDecisionPayload),
    WorkHorizon(WorkHorizonDecisionPayload),
    TaskChange(TaskChangeDecisionPayload),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticDecisionEnvelope {
    pub decision_ref: SemanticDecisionEnvelopeId,
    pub host_session_id: HostSessionId,
    pub host_message_ref: Option<HostMessageRef>,
    pub managed_task_ref: Option<ManagedTaskRef>,
    pub decision_kind: SemanticDecisionKind,
    pub decision_source: DecisionSource,
    pub rationale: String,
    pub confidence: u8,
    pub source_model_ref: String,
    pub issued_at: Timestamp,
    pub decision_payload: SemanticDecisionPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LegacySemanticDecisionEnvelope {
    pub decision_id: SemanticDecisionEnvelopeId,
    pub host_session_id: HostSessionId,
    pub host_message_ref: Option<HostMessageRef>,
    pub managed_task_ref: Option<ManagedTaskRef>,
    pub interaction_lane: InteractionLane,
    pub managed_task_class: Option<ManagedTaskClass>,
    pub work_horizon: Option<WorkHorizonKind>,
    pub task_activation_reason: Option<TaskActivationReason>,
    #[serde(default)]
    pub task_change_classification: Option<TaskChangeClassification>,
    #[serde(default)]
    pub task_change_execution_surface: Option<ChangeExecutionSurface>,
    #[serde(default)]
    pub task_change_boundary_recommendation: Option<BoundaryRecommendation>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub expected_outcome: Option<String>,
    #[serde(default)]
    pub requirement_items: Vec<RequirementItemDraft>,
    pub workspace_ref: Option<String>,
    pub repo_ref: Option<String>,
    #[serde(default)]
    pub allowed_roots: Vec<String>,
    #[serde(default)]
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
    #[serde(default)]
    pub requirement_items: Vec<RequirementItemDraft>,
    #[serde(default)]
    pub allowed_roots: Vec<String>,
    #[serde(default)]
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
pub struct ControlActionEnvelope {
    pub decision_ref: SemanticDecisionEnvelopeId,
    pub decision_source: DecisionSource,
    pub rationale: String,
    pub confidence: u8,
    pub source_model_ref: String,
    pub issued_at: Timestamp,
    pub action: ControlAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticDecisionBatchEnvelope {
    pub meta: IngressMeta,
    pub host_session_id: HostSessionId,
    pub host_message_ref: Option<HostMessageRef>,
    pub input_ref: String,
    pub source_model_ref: String,
    pub issued_at: Timestamp,
    pub rationale_summary: Option<String>,
    #[serde(default)]
    pub semantic_decisions: Vec<SemanticDecisionEnvelope>,
    pub control_action: Option<ControlActionEnvelope>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn legacy_semantic_decision_envelope_deserializes_task_change_governance_fields() {
        let envelope: LegacySemanticDecisionEnvelope = serde_json::from_value(json!({
            "decision_id": "decision-1",
            "host_session_id": "session-1",
            "host_message_ref": "host-message-1",
            "managed_task_ref": "task-1",
            "interaction_lane": "managed_task_active",
            "managed_task_class": "COMPLEX",
            "work_horizon": "improvement",
            "task_activation_reason": "explicit_start_task",
            "task_change_classification": "same_task_minor",
            "task_change_execution_surface": "future_only",
            "task_change_boundary_recommendation": "absorb_change",
            "title": null,
            "summary": "Update the active task.",
            "expected_outcome": null,
            "requirement_items": [],
            "workspace_ref": null,
            "repo_ref": null,
            "allowed_roots": [],
            "secret_classes": [],
            "confidence": 95,
            "created_at": "2026-03-12T12:00:00Z"
        }))
        .expect("legacy semantic decision envelope");

        assert_eq!(
            envelope.task_change_classification,
            Some(TaskChangeClassification::SameTaskMinor)
        );
        assert_eq!(
            envelope.task_change_execution_surface,
            Some(ChangeExecutionSurface::FutureOnly)
        );
        assert_eq!(
            envelope.task_change_boundary_recommendation,
            Some(BoundaryRecommendation::AbsorbChange)
        );
    }

    #[test]
    fn semantic_decision_batch_deserializes_single_judgment_and_control_action() {
        let batch: SemanticDecisionBatchEnvelope = serde_json::from_value(json!({
            "meta": {
                "ingress_id": "ingress-1",
                "received_at": "2026-03-12T12:00:00Z",
                "causation_id": null,
                "correlation_id": "corr-1",
                "dedupe_window": "PT10M"
            },
            "host_session_id": "session-1",
            "host_message_ref": "host-message-1",
            "input_ref": "host-message-1",
            "source_model_ref": "host-model",
            "issued_at": "2026-03-12T12:00:00Z",
            "rationale_summary": "user asked to revise the current task",
            "semantic_decisions": [
                {
                    "decision_ref": "decision-interaction",
                    "host_session_id": "session-1",
                    "host_message_ref": "host-message-1",
                    "managed_task_ref": "task-1",
                    "decision_kind": "interaction_lane",
                    "decision_source": "host_model",
                    "rationale": "the turn targets the active task",
                    "confidence": 95,
                    "source_model_ref": "host-model",
                    "issued_at": "2026-03-12T12:00:00Z",
                    "decision_payload": {
                        "interaction_lane": "managed_task_active",
                        "managed_task_ref": "task-1",
                        "title": null,
                        "summary": null,
                        "expected_outcome": null,
                        "requirement_items": [],
                        "workspace_ref": null,
                        "repo_ref": null,
                        "allowed_roots": [],
                        "secret_classes": []
                    }
                },
                {
                    "decision_ref": "decision-task-change",
                    "host_session_id": "session-1",
                    "host_message_ref": "host-message-1",
                    "managed_task_ref": "task-1",
                    "decision_kind": "task_change",
                    "decision_source": "host_model",
                    "rationale": "future-only minor change",
                    "confidence": 88,
                    "source_model_ref": "host-model",
                    "issued_at": "2026-03-12T12:00:00Z",
                    "decision_payload": {
                        "classification": "same_task_minor",
                        "execution_surface": "future_only",
                        "boundary_recommendation": "absorb_change"
                    }
                }
            ],
            "control_action": {
                "decision_ref": "decision-control",
                "decision_source": "user_control_action",
                "rationale": "explicit task change request",
                "confidence": 99,
                "source_model_ref": "host-model",
                "issued_at": "2026-03-12T12:00:00Z",
                "action": {
                    "action_id": "action-1",
                    "managed_task_ref": "task-1",
                    "kind": "request_task_change",
                    "actor": "user",
                    "payload": {
                        "title": null,
                        "summary": "Expand the task",
                        "expected_outcome": "Task scope includes retries",
                        "requirement_items": [],
                        "allowed_roots": [],
                        "secret_classes": [],
                        "workspace_ref": null,
                        "repo_ref": null,
                        "rationale": null
                    },
                    "source_decision_ref": "decision-task-change",
                    "decision_token": null
                }
            }
        }))
        .expect("semantic decision batch");

        assert_eq!(batch.semantic_decisions.len(), 2);
        assert!(matches!(
            batch.semantic_decisions[0].decision_payload,
            SemanticDecisionPayload::InteractionLane(_)
        ));
        assert!(matches!(
            batch.semantic_decisions[1].decision_payload,
            SemanticDecisionPayload::TaskChange(_)
        ));
        assert_eq!(
            batch
                .control_action
                .as_ref()
                .expect("control action")
                .action
                .source_decision_ref
                .as_deref(),
            Some("decision-task-change")
        );
    }
}
