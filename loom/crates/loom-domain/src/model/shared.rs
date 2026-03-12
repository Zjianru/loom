use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ManagedTaskClass {
    Complex,
    Huge,
    Max,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkHorizonKind {
    Maintenance,
    Improvement,
    Extension,
    Disruption,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskActivationReason {
    #[serde(
        alias = "explicit_user_request",
        alias = "scope_change",
        alias = "capability_drift",
        alias = "review_escalation"
    )]
    ExplicitStartTask,
    ExplicitTrackThis,
    DelegateHeavyWork,
    HeavyMultiStageGoal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStage {
    Candidate,
    Execute,
    Review,
    Result,
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InteractionLane {
    Chat,
    ManagedTaskCandidate,
    ManagedTaskActive,
}

#[cfg(test)]
mod tests {
    use super::TaskActivationReason;
    use crate::{
        KernelOutboundPayload, ManagedTask, ManagedTaskClass, StartCardPayload, WorkHorizonKind,
    };
    use serde_json::json;

    #[test]
    fn managed_task_deserializes_legacy_activation_reason_values() {
        let payload = json!({
            "managed_task_ref": "task-1",
            "host_session_id": "agent:main:main",
            "managed_task_class": "COMPLEX",
            "work_horizon": "improvement",
            "activation_reason": "explicit_user_request",
            "workflow_stage": "candidate",
            "title": "Managed task",
            "summary": "Legacy runtime row",
            "expected_outcome": "Still readable after enum rename",
            "workspace_ref": null,
            "repo_ref": null,
            "allowed_roots": [],
            "secret_classes": [],
            "requirement_items": [],
            "current_scope_ref": null,
            "current_scope_version": null,
            "current_baseline_risk_ref": null,
            "current_execution_authorization_ref": null,
            "current_pending_window_ref": "window-1",
            "active_run_ref": null,
            "spec_bundle": null,
            "phase_plan": null,
            "agent_binding": null,
            "review_result": null,
            "proof_of_work_bundle": null,
            "result_contract": null,
            "created_at": "1",
            "updated_at": "1"
        });

        let task: ManagedTask = serde_json::from_value(payload).expect("managed task");
        assert_eq!(
            task.activation_reason,
            TaskActivationReason::ExplicitStartTask
        );
    }

    #[test]
    fn start_card_payload_deserializes_legacy_activation_reason_values() {
        let payload = json!({
            "start_card": {
                "managed_task_ref": "task-1",
                "decision_token": "decision-1",
                "managed_task_class": "COMPLEX",
                "work_horizon": "improvement",
                "task_activation_reason": "explicit_user_request",
                "title": "Managed task",
                "summary": "Legacy start card",
                "expected_outcome": "Still readable after enum rename",
                "recommended_pack_ref": "coding_pack",
                "allowed_actions": ["approve_start", "modify_candidate", "cancel_candidate"],
                "render_hint": {
                    "tone": "neutral",
                    "emphasis": "standard",
                    "host_text_mode": "structured_text"
                }
            }
        });

        let outbound: KernelOutboundPayload =
            serde_json::from_value(payload).expect("kernel outbound payload");
        let KernelOutboundPayload::StartCard(StartCardPayload {
            task_activation_reason,
            managed_task_class,
            work_horizon,
            ..
        }) = outbound
        else {
            panic!("expected start card payload");
        };
        assert_eq!(
            task_activation_reason,
            TaskActivationReason::ExplicitStartTask
        );
        assert_eq!(managed_task_class, ManagedTaskClass::Complex);
        assert_eq!(work_horizon, WorkHorizonKind::Improvement);
    }
}
