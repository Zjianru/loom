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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum HostKind {
    #[default]
    #[serde(rename = "openclaw")]
    OpenClaw,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct HostAgentCapability {
    pub host_agent_ref: String,
    pub display_name: String,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct HostModelCapability {
    pub host_model_ref: String,
    pub provider: String,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct HostToolCapability {
    pub tool_name: String,
    pub available: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum HostSpawnRuntimeKind {
    #[default]
    Subagent,
    Acp,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum HostSpawnAgentScopeMode {
    All,
    ExplicitList,
    None,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct HostSpawnAgentScope {
    #[serde(default)]
    pub mode: HostSpawnAgentScopeMode,
    #[serde(default)]
    pub allowed_host_agent_refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct HostSpawnCapability {
    pub runtime_kind: HostSpawnRuntimeKind,
    pub available: bool,
    #[serde(default)]
    pub host_agent_scope: HostSpawnAgentScope,
    pub supports_resume_session: bool,
    pub supports_thread_spawn: bool,
    pub supports_parent_progress_stream: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum HostSessionRole {
    Main,
    Orchestrator,
    Leaf,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum HostSessionControlScope {
    Children,
    None,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum HostCapabilityFactSource {
    Authoritative,
    Derived,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct HostSessionCapabilityScope {
    pub session_role: HostSessionRole,
    pub control_scope: HostSessionControlScope,
    #[serde(default)]
    pub source: HostCapabilityFactSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct HostRenderCapabilities {
    pub supports_text_render: bool,
    pub supports_inline_actions: bool,
    pub supports_message_suppression: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostCapabilitySnapshot {
    pub capability_snapshot_ref: HostCapabilitySnapshotRef,
    #[serde(default)]
    pub host_kind: HostKind,
    pub host_session_id: HostSessionId,
    #[serde(default)]
    pub available_agents: Vec<HostAgentCapability>,
    #[serde(default)]
    pub available_models: Vec<HostModelCapability>,
    #[serde(default)]
    pub available_tools: Vec<HostToolCapability>,
    #[serde(default)]
    pub spawn_capabilities: Vec<HostSpawnCapability>,
    #[serde(default)]
    pub session_scope: HostSessionCapabilityScope,
    pub allowed_tools: Vec<String>,
    pub readable_roots: Vec<String>,
    pub writable_roots: Vec<String>,
    pub secret_classes: Vec<String>,
    pub max_budget_band: AuthorizationBudgetBand,
    #[serde(default)]
    pub render_capabilities: HostRenderCapabilities,
    #[serde(default)]
    pub background_task_support: bool,
    #[serde(default)]
    pub async_notice_support: bool,
    #[serde(default)]
    pub available_agent_ids: Vec<String>,
    #[serde(default)]
    pub supports_spawn_agents: bool,
    #[serde(default)]
    pub supports_pause: bool,
    #[serde(default)]
    pub supports_resume: bool,
    #[serde(default)]
    pub supports_interrupt: bool,
    #[serde(default)]
    pub worker_control_capabilities: HostWorkerControlCapabilities,
    pub recorded_at: Timestamp,
}

impl HostCapabilitySnapshot {
    pub fn spawn_capability(
        &self,
        runtime_kind: HostSpawnRuntimeKind,
    ) -> Option<&HostSpawnCapability> {
        self.spawn_capabilities
            .iter()
            .find(|capability| capability.runtime_kind == runtime_kind)
    }

    pub fn supports_spawn_runtime(&self, runtime_kind: HostSpawnRuntimeKind) -> bool {
        self.spawn_capability(runtime_kind)
            .map(|capability| capability.available)
            .unwrap_or(
                matches!(runtime_kind, HostSpawnRuntimeKind::Subagent)
                    && self.supports_spawn_agents,
            )
    }

    pub fn allowed_spawn_agent_refs(&self, runtime_kind: HostSpawnRuntimeKind) -> Vec<String> {
        self.spawn_capability(runtime_kind)
            .map(|capability| match capability.host_agent_scope.mode {
                HostSpawnAgentScopeMode::All => {
                    let available_agents = self
                        .available_agents
                        .iter()
                        .map(|agent| agent.host_agent_ref.clone())
                        .collect::<Vec<_>>();
                    if available_agents.is_empty() {
                        self.available_agent_ids.clone()
                    } else {
                        available_agents
                    }
                }
                HostSpawnAgentScopeMode::ExplicitList => {
                    capability.host_agent_scope.allowed_host_agent_refs.clone()
                }
                HostSpawnAgentScopeMode::None | HostSpawnAgentScopeMode::Unknown => Vec::new(),
            })
            .unwrap_or_else(|| {
                if matches!(runtime_kind, HostSpawnRuntimeKind::Subagent) {
                    self.available_agent_ids.clone()
                } else {
                    Vec::new()
                }
            })
    }

    pub fn spawn_runtime_allows_agent(
        &self,
        runtime_kind: HostSpawnRuntimeKind,
        host_agent_ref: &str,
    ) -> bool {
        self.spawn_capability(runtime_kind)
            .map(|capability| {
                capability.available
                    && match capability.host_agent_scope.mode {
                        HostSpawnAgentScopeMode::All => true,
                        HostSpawnAgentScopeMode::ExplicitList => capability
                            .host_agent_scope
                            .allowed_host_agent_refs
                            .iter()
                            .any(|allowed| allowed == host_agent_ref),
                        HostSpawnAgentScopeMode::None | HostSpawnAgentScopeMode::Unknown => false,
                    }
            })
            .unwrap_or_else(|| {
                matches!(runtime_kind, HostSpawnRuntimeKind::Subagent)
                    && self.supports_spawn_agents
                    && self
                        .available_agent_ids
                        .iter()
                        .any(|allowed| allowed == host_agent_ref)
            })
    }

    pub fn session_scope_is_authoritative(&self) -> bool {
        self.session_scope.source == HostCapabilityFactSource::Authoritative
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct HostWorkerControlCapabilities {
    pub supports_pause: bool,
    pub supports_resume: bool,
    pub supports_cancel: bool,
    pub supports_soft_interrupt: bool,
    pub supports_hard_interrupt: bool,
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
    fn host_kind_deserializes_openclaw_wire_value() {
        let host_kind: HostKind = serde_json::from_str("\"openclaw\"").expect("host kind");
        assert_eq!(host_kind, HostKind::OpenClaw);
    }

    #[test]
    fn capability_snapshot_deserializes_spawn_scope_and_session_scope_source() {
        let snapshot: HostCapabilitySnapshot = serde_json::from_value(json!({
            "capability_snapshot_ref": "cap-1",
            "host_kind": "openclaw",
            "host_session_id": "agent:main:main",
            "available_agents": [],
            "available_models": [],
            "available_tools": [],
            "spawn_capabilities": [
                {
                    "runtime_kind": "acp",
                    "available": true,
                    "host_agent_scope": {
                        "mode": "all",
                        "allowed_host_agent_refs": []
                    },
                    "supports_resume_session": true,
                    "supports_thread_spawn": true,
                    "supports_parent_progress_stream": true
                }
            ],
            "session_scope": {
                "session_role": "main",
                "control_scope": "children",
                "source": "derived"
            },
            "allowed_tools": [],
            "readable_roots": [],
            "writable_roots": [],
            "secret_classes": [],
            "max_budget_band": "standard",
            "render_capabilities": {
                "supports_text_render": true,
                "supports_inline_actions": false,
                "supports_message_suppression": true
            },
            "background_task_support": false,
            "async_notice_support": false,
            "available_agent_ids": [],
            "supports_spawn_agents": false,
            "supports_pause": false,
            "supports_resume": false,
            "supports_interrupt": false,
            "worker_control_capabilities": {
                "supports_pause": false,
                "supports_resume": false,
                "supports_cancel": false,
                "supports_soft_interrupt": false,
                "supports_hard_interrupt": false
            },
            "recorded_at": "2026-03-12T12:00:00Z"
        }))
        .expect("capability snapshot");

        assert_eq!(
            snapshot.spawn_capabilities[0].host_agent_scope.mode,
            HostSpawnAgentScopeMode::All
        );
        assert_eq!(
            snapshot.session_scope.source,
            HostCapabilityFactSource::Derived
        );
    }

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
