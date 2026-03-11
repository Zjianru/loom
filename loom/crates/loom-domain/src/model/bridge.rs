use crate::model::ids::{
    AdapterId, AgentBindingId, BridgeBootstrapSecret, BridgeBootstrapTicketId, BridgeCredentialId,
    BridgeInstanceId, BridgeNonce, BridgeSecretRef, BridgeSessionSecret, BridgeSignature,
    DecisionToken, HostExecutionCommandId, HostSessionId, IsolatedTaskRunRef, ManagedTaskRef,
    Timestamp,
};
use crate::model::ingress::ControlActionKind;
use crate::model::task::AgentRoleKind;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BridgeCredentialStatus {
    PendingBootstrap,
    Active,
    Rotating,
    Revoked,
    Expired,
    Consumed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BridgeBootstrapTicket {
    pub ticket_id: BridgeBootstrapTicketId,
    pub bridge_instance_id: BridgeInstanceId,
    pub adapter_id: AdapterId,
    pub issued_at: Timestamp,
    pub expires_at: Timestamp,
    pub ticket_secret_hash: String,
    pub status: BridgeCredentialStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BridgeBootstrapMaterial {
    pub bridge_instance_id: BridgeInstanceId,
    pub adapter_id: AdapterId,
    pub ticket_id: BridgeBootstrapTicketId,
    pub ticket_secret: BridgeBootstrapSecret,
    pub issued_at: Timestamp,
    pub expires_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BridgeSessionCredential {
    pub credential_id: BridgeCredentialId,
    pub bridge_instance_id: BridgeInstanceId,
    pub adapter_id: AdapterId,
    pub secret_ref: BridgeSecretRef,
    pub secret_hash: String,
    pub rotation_epoch: u32,
    pub issued_at: Timestamp,
    pub expires_at: Option<Timestamp>,
    pub status: BridgeCredentialStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BridgeAuthEnvelope {
    pub bridge_instance_id: BridgeInstanceId,
    pub adapter_id: AdapterId,
    pub secret_ref: BridgeSecretRef,
    pub rotation_epoch: u32,
    pub signed_at: Timestamp,
    pub nonce: BridgeNonce,
    pub signature: BridgeSignature,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BridgeBootstrapRequest {
    pub bridge_instance_id: BridgeInstanceId,
    pub adapter_id: AdapterId,
    pub ticket_id: BridgeBootstrapTicketId,
    pub ticket_secret: BridgeBootstrapSecret,
    pub requested_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BridgeBootstrapAck {
    pub bridge_instance_id: BridgeInstanceId,
    pub credential_id: BridgeCredentialId,
    pub secret_ref: BridgeSecretRef,
    pub rotation_epoch: u32,
    pub session_secret: BridgeSessionSecret,
    pub issued_at: Timestamp,
    pub expires_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BridgeHealthResponse {
    pub bridge_instance_id: BridgeInstanceId,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ControlSurfaceType {
    StartCard,
    BoundaryCard,
    ApprovalRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CurrentControlSurfaceProjection {
    pub host_session_id: HostSessionId,
    pub surface_type: ControlSurfaceType,
    pub managed_task_ref: ManagedTaskRef,
    pub decision_token: DecisionToken,
    pub allowed_actions: Vec<ControlActionKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostExecutionCommand {
    pub command_id: HostExecutionCommandId,
    pub managed_task_ref: ManagedTaskRef,
    pub run_ref: IsolatedTaskRunRef,
    pub binding_id: AgentBindingId,
    pub role_kind: AgentRoleKind,
    pub host_session_id: HostSessionId,
    pub host_agent_id: String,
    pub prompt: String,
    pub label: String,
    pub status: HostExecutionCommandStatus,
    #[serde(default, alias = "child_session_key")]
    pub host_child_execution_ref: Option<String>,
    #[serde(default, alias = "child_run_id")]
    pub host_child_run_ref: Option<String>,
    pub output_summary: Option<String>,
    pub artifact_refs: Vec<String>,
    pub issued_at: Timestamp,
    pub acked_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HostExecutionCommandStatus {
    Pending,
    Dispatched,
    Running,
    Completed,
    Failed,
}
