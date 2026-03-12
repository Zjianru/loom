use crate::model::ids::{
    ExecutionAuthorizationId, ExecutionAuthorizationRef, HostCapabilitySnapshotRef,
    IsolatedTaskRunRef, ManagedTaskRef, StageRunRef, TaskScopeId, Timestamp,
};
use crate::model::ingress::HostSpawnRuntimeKind;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionAuthorization {
    pub authorization_id: ExecutionAuthorizationId,
    pub managed_task_ref: ManagedTaskRef,
    pub run_ref: IsolatedTaskRunRef,
    pub stage_run_ref: Option<StageRunRef>,
    pub capability_snapshot_ref: HostCapabilitySnapshotRef,
    pub task_scope_ref: TaskScopeId,
    pub granted_areas: Vec<AuthorizedDecisionArea>,
    pub status: ExecutionAuthorizationStatus,
    pub issued_reason: String,
    pub issued_at: Timestamp,
    pub expires_at: Option<Timestamp>,
    pub supersedes: Option<ExecutionAuthorizationRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthorizedDecisionArea {
    pub decision_area: DecisionArea,
    pub band: DelegationBand,
    pub allowed_tools: Vec<String>,
    pub readable_roots: Vec<String>,
    pub writable_roots: Vec<String>,
    pub allowed_secret_classes: Vec<String>,
    pub not_before: Option<Timestamp>,
    pub not_after: Option<Timestamp>,
    pub budget_band: AuthorizationBudgetBand,
    pub authorized_spawn_capabilities: Vec<AuthorizedSpawnCapability>,
    pub requires_user_approval: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthorizedSpawnCapability {
    pub runtime_kind: HostSpawnRuntimeKind,
    pub host_agent_scope: AuthorizedSpawnAgentScope,
    pub supports_resume_session: bool,
    pub supports_thread_spawn: bool,
    pub supports_parent_progress_stream: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthorizedSpawnAgentScope {
    pub mode: AuthorizedSpawnAgentScopeMode,
    pub allowed_host_agent_refs: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthorizedSpawnAgentScopeMode {
    All,
    ExplicitList,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecisionArea {
    TaskExecution,
    ToolExecution,
    TaskChangeConfirmation,
    CapabilityDriftResolution,
    ReviewResolution,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DelegationBand {
    Autonomous,
    Guarded,
    UserMediated,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum AuthorizationBudgetBand {
    Conservative,
    Standard,
    Elevated,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionAuthorizationStatus {
    Draft,
    Active,
    Suspended,
    Narrowed,
    Revoked,
    Expired,
    Superseded,
}
