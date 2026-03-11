use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

pub type Timestamp = String;
pub type ManagedTaskRef = String;
pub type DecisionToken = String;
pub type PendingDecisionWindowId = String;
pub type PendingDecisionWindowRef = String;
pub type TaskScopeId = String;
pub type PhasePlanId = String;
pub type PhasePlanEntryId = String;
pub type RiskAssessmentId = String;
pub type RiskAssessmentRef = String;
pub type ExecutionAuthorizationId = String;
pub type ExecutionAuthorizationRef = String;
pub type HostCapabilitySnapshotRef = String;
pub type IsolatedTaskRunRef = String;
pub type AgentBindingId = String;
pub type AgentBindingRef = String;
pub type ProofOfWorkBundleId = String;
pub type HostExecutionCommandId = String;
pub type OutboundDeliveryId = String;
pub type ControlActionId = String;
pub type SemanticDecisionEnvelopeId = String;
pub type HostSessionId = String;
pub type HostMessageRef = String;
pub type RequirementId = String;
pub type StageRunRef = String;
pub type PackRef = String;
pub type SpecBundleId = String;
pub type ReviewResultId = String;
pub type ResultContractId = String;
pub type BridgeInstanceId = String;
pub type BridgeBootstrapTicketId = String;
pub type BridgeCredentialId = String;
pub type BridgeSecretRef = String;
pub type BridgeNonce = String;
pub type BridgeSignature = String;
pub type BridgeBootstrapSecret = String;
pub type BridgeSessionSecret = String;
pub type AdapterId = String;

pub fn new_id(prefix: &str) -> String {
    format!("{prefix}-{}", Uuid::new_v4())
}

pub fn now_timestamp() -> Timestamp {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}
